use crate::action::EventAction;
use crate::errors::{LaikaError, LaikaResult};
use crate::event::{CorrelatedEvent, Event, EventLike};
use std::collections::HashMap;
use time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventType {
    UnknownEvent,
    KnownEvent(String), // EventName
}

impl EventProcessorGroup {
    fn match_event_type(&self, event: &Event) -> LaikaResult<EventType> {
        let event_type = event.event_type()?;
        Ok(self
            .event_matcher
            .get(&event_type)
            .map_or(EventType::UnknownEvent, |name| {
                EventType::KnownEvent(name.to_string())
            }))
    }

    /// Actions to take given matched conditions, if any
    ///
    /// Assumes all events passed share a correlation ID.
    pub fn matched_actions(&self, events: &[Event]) -> LaikaResult<Vec<EventAction>> {
        let event_types: Vec<EventType> = events
            .iter()
            .map(|event| self.match_event_type(event))
            .collect::<LaikaResult<Vec<EventType>>>()?;

        let actions: Vec<EventAction> = self
            .rules
            .iter()
            .filter_map(|(rule, action)| {
                rule.is_satisfied(event_types.as_slice(), events)
                    .map(|_| action.clone())
                    .ok()
            })
            .collect();

        Ok(actions)
    }
}

#[derive(Clone)]
pub enum Condition {
    TimingCondition(TimingCondition),
}

impl Condition {
    pub(crate) fn is_satisfied<'a>(
        &self,
        events: impl Iterator<Item = (&'a EventType, &'a Event)>,
    ) -> bool {
        match self {
            Condition::TimingCondition(timing_condition) => {
                let mut event_times = vec![];
                let mut maybe_target_event_time = None;
                for (event_type, event_data) in events {
                    event_times.push(event_data.received());
                    if let EventType::KnownEvent(event_name) = event_type {
                        if event_name.as_str() == timing_condition.event {
                            maybe_target_event_time = Some(event_data.received().clone());
                        }
                    }
                }
                match maybe_target_event_time {
                    None => false,
                    Some(target_event_time) => {
                        match timing_condition.start_from {
                            TimingEvent::FirstEvent => {
                                // There's at least one value from target_event_time
                                let start_time = event_times.iter().min().unwrap();
                                (target_event_time - **start_time) <= timing_condition.within
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub enum TimingEvent {
    FirstEvent,
}

#[derive(Clone)]
pub struct TimingCondition {
    event: String,
    within: Duration,
    start_from: TimingEvent,
}

#[derive(Clone)]
pub struct EventProcessorGroup {
    pub event_matcher: HashMap<String, String>, // EventType -> EventName
    pub rules: Vec<(EventRule, EventAction)>,
}

#[derive(Clone)]
pub struct EventRule {
    name: String,
    condition: Option<Condition>,
    condition_inverted: bool,
    requires: Vec<String>,
}

impl EventRule {
    fn valid_correlation<'a>(&self, events: impl Iterator<Item = &'a Event>) -> bool {
        let mut n_events: usize = 0;
        for event in events {
            if matches!(event, Event::NonCorrelated(_)) && n_events > 0 {
                return false;
            }
            n_events += 1;
        }
        true
    }

    fn meets_requirements<'a>(&self, event_type: impl Iterator<Item = &'a EventType>) -> bool {
        if self.requires.is_empty() {
            return true;
        }

        let mut found = 0;
        let required_count = self.requires.len();

        for event in event_type {
            if let EventType::KnownEvent(event_name) = event {
                if self.requires.contains(event_name) {
                    found += 1;
                    if found == required_count {
                        return true;
                    }
                }
            }
        }

        found == required_count
    }
    pub fn is_satisfied<'a>(
        &self,
        event_type: &'a [EventType],
        event_data: &'a [Event],
    ) -> LaikaResult<bool> {
        if !self.valid_correlation(event_data.iter()) {
            return Err(LaikaError::InvalidEventGroup);
        }
        let meets_requirements = self.meets_requirements(event_type.iter());
        if let Some(condition) = &self.condition {
            // Inverted | Condition - XOR
            // T T => F
            // T F => T
            // F T => T
            // F F => F
            Ok(meets_requirements
                && (self.condition_inverted
                    ^ condition.is_satisfied(event_type.iter().zip(event_data))))
        } else {
            Ok(meets_requirements)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::LaikaResult;
    use crate::event::RawEvent;
    use crate::rules::Condition::TimingCondition as ConditionType;
    use serde_json::json;
    use std::time::SystemTime;
    use time::OffsetDateTime;

    fn event_a() -> LaikaResult<Event> {
        RawEvent::new(
            json!({
                "type": "PaymentInitiated",
                "transaction_id": 1,
                "customer_id": 123,
                "value": 100.50,
                "currency": "USD"
            }), // Correlation Target should be provided by the original YAML
        )
        .with_correlation_target("transaction_id")
    }

    fn event_a_non_correlated() -> LaikaResult<Event> {
        RawEvent::new(
            json!({
                "type": "PaymentInitiated",
                "transaction_id": 1,
                "customer_id": 123,
                "value": 100.50,
                "currency": "USD"
            }), // Correlation Target should be provided by the original YAML
        )
        .without_correlation_id()
    }

    fn event_b() -> LaikaResult<Event> {
        RawEvent::new(
            json!({
                "type": "PaymentAuthorised",
                "txn_id": 1,
                "customer_id": 123
            }), // Correlation Target should be provided by the original YAML
        )
        .with_correlation_target("txn_id")
    }

    fn event_b_non_correlated() -> LaikaResult<Event> {
        RawEvent::new(
            json!({
                "type": "PaymentAuthorised",
                "txn_id": 1,
                "customer_id": 123
            }), // Correlation Target should be provided by the original YAML
        )
        .without_correlation_id()
    }

    fn event_c() -> LaikaResult<Event> {
        RawEvent::new(json!({
            "type": "PaymentSettled",
            "transaction_id": 1,
            "customer_id": 123
        }))
        .with_correlation_target("transaction_id")
    }

    fn create_event_with_time(
        event_type: &str,
        transaction_id: i64,
        timestamp: SystemTime,
    ) -> LaikaResult<Event> {
        let mut event = RawEvent::new(json!({
            "type": event_type,
            "transaction_id": transaction_id,
            "customer_id": 123,
            "value": 100.50,
            "currency": "USD"
        }))
        .with_correlation_target("transaction_id")?;
        event.set_received(OffsetDateTime::from(timestamp));
        Ok(event)
    }

    fn default_event_matcher() -> HashMap<String, String> {
        let mut event_matcher = HashMap::new();
        event_matcher.insert("PaymentInitiated".to_string(), "eventA".to_string());
        event_matcher.insert("PaymentAuthorised".to_string(), "eventB".to_string());
        event_matcher.insert("PaymentSettled".to_string(), "eventC".to_string());
        event_matcher
    }

    #[test]
    fn test_event_matcher() -> LaikaResult<()> {
        let group = EventProcessorGroup {
            event_matcher: default_event_matcher(),
            rules: vec![],
        };
        for (event, event_name) in vec![
            (event_a()?, "eventA".to_string()),
            (event_b()?, "eventB".to_string()),
            (event_c()?, "eventC".to_string()),
        ] {
            assert_eq!(
                group.match_event_type(&event)?,
                EventType::KnownEvent(event_name)
            );
        }
        Ok(())
    }

    #[test]
    fn test_unknown_event_type() -> LaikaResult<()> {
        let group = EventProcessorGroup {
            event_matcher: default_event_matcher(),
            rules: vec![],
        };

        let unknown_event = RawEvent::new(json!({
            "type": "UnknownEventType",
            "transaction_id": 1
        }))
        .with_correlation_target("transaction_id")?;

        assert_eq!(
            group.match_event_type(&unknown_event)?,
            EventType::UnknownEvent
        );
        Ok(())
    }

    #[test]
    fn test_partial_requirements_not_satisfied() -> LaikaResult<()> {
        let rule = EventRule {
            name: "partialRule".to_string(),
            condition_inverted: false,
            condition: None,
            requires: vec![
                "eventA".to_string(),
                "eventB".to_string(),
                "eventC".to_string(),
            ],
        };

        // Only two events present when three are required
        let events = vec![event_a()?, event_b()?];
        let event_types = vec![
            EventType::KnownEvent("eventA".to_string()),
            EventType::KnownEvent("eventB".to_string()),
        ];

        assert!(!rule.is_satisfied(event_types.as_slice(), &events)?);
        Ok(())
    }

    #[test]
    fn test_raise_invalid_event_group() -> LaikaResult<()> {
        let rule = EventRule {
            name: "NoCorrelation".to_string(),
            condition_inverted: false,
            condition: None,
            requires: vec!["eventA".to_string(), "eventB".to_string()],
        };

        // EventA NonCorrelated is the only item
        let events = vec![event_a_non_correlated()?, event_b_non_correlated()?];
        let event_types = vec![
            EventType::KnownEvent("eventA".to_string()),
            EventType::KnownEvent("eventB".to_string()),
        ];

        assert!(matches!(
            rule.is_satisfied(event_types.as_slice(), &events),
            Err(LaikaError::InvalidEventGroup)
        ));
        Ok(())
    }

    #[test]
    fn test_conditionless_event_rule() -> LaikaResult<()> {
        let group = EventProcessorGroup {
            event_matcher: default_event_matcher(),
            rules: vec![],
        };
        let rule = EventRule {
            name: "successRule".to_string(),
            condition: None,
            condition_inverted: false,
            requires: vec![
                "eventA".to_string(),
                "eventB".to_string(),
                "eventC".to_string(),
            ],
        };
        let events = vec![event_a()?, event_b()?, event_c()?];
        let event_types = vec![
            EventType::KnownEvent("eventA".to_string()),
            EventType::KnownEvent("eventB".to_string()),
            EventType::KnownEvent("eventC".to_string()),
        ];
        assert!(rule.is_satisfied(event_types.as_slice(), &events)?);
        Ok(())
    }

    #[test]
    // Test rule is not satisfied if timing is exceeded
    fn test_timing_condition_exceeded() -> LaikaResult<()> {
        let base_time = SystemTime::now();
        let rule = EventRule {
            name: "timingRule".to_string(),
            condition: Some(ConditionType(TimingCondition {
                event: "eventC".to_string(),
                within: Duration::milliseconds(500),
                start_from: TimingEvent::FirstEvent,
            })),
            condition_inverted: false,
            requires: vec![
                "eventA".to_string(),
                "eventB".to_string(),
                "eventC".to_string(),
            ],
        };

        // Create events with the last event occurring after the timing window
        let events = vec![
            create_event_with_time("PaymentInitiated", 1, base_time)?,
            create_event_with_time(
                "PaymentAuthorised",
                1,
                base_time + Duration::milliseconds(200),
            )?,
            create_event_with_time("PaymentSettled", 1, base_time + Duration::milliseconds(600))?, // > 500ms window
        ];

        let event_types = vec![
            EventType::KnownEvent("eventA".to_string()),
            EventType::KnownEvent("eventB".to_string()),
            EventType::KnownEvent("eventC".to_string()),
        ];

        assert!(
            !rule.is_satisfied(event_types.as_slice(), &events)?,
            "Rule should not be satisfied when timing condition is exceeded"
        );
        Ok(())
    }

    #[test]
    // Test rule is satisfied when it has no requirements
    fn test_empty_requirements() -> LaikaResult<()> {
        let rule = EventRule {
            name: "emptyRule".to_string(),
            condition: None,
            condition_inverted: false,
            requires: vec![],
        };

        let events = vec![event_a()?];
        let event_types = vec![EventType::KnownEvent("eventA".to_string())];

        assert!(
            rule.is_satisfied(event_types.as_slice(), &events)?,
            "Rule with empty requirements should always be satisfied"
        );
        Ok(())
    }

    #[test]
    fn test_timing_condition_with_different_intervals() -> LaikaResult<()> {
        let base_time = SystemTime::now();

        // Test cases with different time intervals
        let test_cases = vec![
            (Duration::seconds(1), true),
            (Duration::milliseconds(500), true),
            (Duration::milliseconds(50), false),
        ];

        for (duration, expected_result) in test_cases {
            let rule = EventRule {
                name: "timingRule".to_string(),
                condition: Some(ConditionType(TimingCondition {
                    event: "eventC".to_string(),
                    within: duration,
                    start_from: TimingEvent::FirstEvent,
                })),
                condition_inverted: false,
                requires: vec![
                    "eventA".to_string(),
                    "eventB".to_string(),
                    "eventC".to_string(),
                ],
            };

            let events = vec![
                create_event_with_time("PaymentInitiated", 1, base_time)?,
                create_event_with_time(
                    "PaymentAuthorised",
                    1,
                    base_time + Duration::milliseconds(100),
                )?,
                create_event_with_time(
                    "PaymentSettled",
                    1,
                    base_time + Duration::milliseconds(200),
                )?,
            ];

            let event_types = vec![
                EventType::KnownEvent("eventA".to_string()),
                EventType::KnownEvent("eventB".to_string()),
                EventType::KnownEvent("eventC".to_string()),
            ];

            assert_eq!(
                rule.is_satisfied(event_types.as_slice(), &events)?,
                expected_result,
                "Failed for duration {:?}",
                duration
            );
        }
        Ok(())
    }
}
