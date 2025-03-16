use crate::config::builder::{ActionConfig, RequirementConfig, TimingConfig};
use crate::errors::{LaikaError, LaikaResult};
use crate::event::context::EventContext;
use crate::event::{Event, Trigger};
use crate::predicate_engine::{JsonPredicate, JsonPredicateEngine};
use time::OffsetDateTime;
use tracing::error;

#[derive(Debug)]
pub enum RuleResult {
    /// The rule's requirements were met and its condition evaluated to true
    ConditionSatisfied {
        met_at: OffsetDateTime,
        action_config: ActionConfig,
        condition_result: serde_json::Value,
    },
    /// The rule's requirements were met but its condition evaluated to false
    ConditionNotSatisfied {
        met_at: OffsetDateTime,
        // If a recheck is defined within the rule
        recheck: Option<TimingConfig>,
    },
    /// The rule's requirements were not met, so the condition wasn't evaluated, even if the rule's condition was empty
    RequirementNotMet {},
}

impl From<RequirementConfig> for Requirement {
    fn from(value: RequirementConfig) -> Self {
        match value {
            RequirementConfig::Exact { exact } => Requirement::Exactly(exact),
            RequirementConfig::AtLeast { at_least } => Requirement::AtLeast(at_least),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Requirement {
    AtLeast(Vec<String>),
    Exactly(Vec<String>),
}

impl Requirement {
    pub fn len(&self) -> usize {
        match self {
            Requirement::Exactly(exact) => exact.len(),
            Requirement::AtLeast(at_least) => at_least.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone)]
pub struct EventRule {
    pub(crate) name: String,
    // EventRules will always have a default JsonPredicate, even if the user hasn't provided one.
    pub(crate) filter_and_extract: JsonPredicate,
    pub(crate) timing: Option<TimingConfig>,
    pub(crate) requires: Option<Requirement>,
    pub(crate) action: ActionConfig,
}

impl EventRule {
    fn valid_correlation(&self, trigger: &Trigger, context: &EventContext) -> bool {
        let minimum_events: usize = self
            .requires
            .as_ref()
            .map(|req| match req {
                Requirement::AtLeast(reqs) | Requirement::Exactly(reqs) => reqs.len(),
            })
            .unwrap_or(0);
        let mut count = 0;
        for event in context.events() {
            // NonCorrelated events must be alone.
            if matches!(event, Event::NonCorrelated(_)) && count > 0 {
                return false;
            }
            count += 1;
        }
        if let Trigger::ReceivedEvent(Event::NonCorrelated(_)) = trigger {
            return count == 0 && // Can only add NonCorrelated to empty list
                minimum_events <= 1; // If we require more than 1 event, the events must be correlated.
        }
        true
    }

    fn when_met_requirements(
        &self,
        trigger: &Trigger,
        context: &EventContext,
    ) -> Option<OffsetDateTime> {
        let mut event_with_types: Vec<(Event, String)> = context
            .events()
            .map(|e| (e.event_type().expect("All events have types"), e))
            .map(|(a, b)| (b.clone(), a))
            .collect();
        if let Trigger::ReceivedEvent(e) = trigger {
            event_with_types.push((e.clone(), e.event_type().expect("Trigger must have a type")));
        }
        match &self.requires {
            None => event_with_types.last().map(|(e, _)| e.received().clone()),
            Some(requirement) => match requirement {
                Requirement::AtLeast(targets) => {
                    let mut met_targets = std::collections::HashSet::new();

                    for (event, event_type) in event_with_types {
                        if targets.contains(&event_type) {
                            met_targets.insert(event_type);
                        }

                        if met_targets.len() >= targets.len() {
                            return Some(event.received().clone());
                        }
                    }
                    None
                }
                Requirement::Exactly(targets) => {
                    if event_with_types.len() != targets.len() {
                        return None;
                    }

                    let event_type_set: std::collections::HashSet<_> =
                        event_with_types.iter().map(|(_, t)| t).collect();

                    let targets_set: std::collections::HashSet<_> = targets.iter().collect();

                    if event_type_set == targets_set {
                        event_with_types.last().map(|(e, _)| e.received().clone())
                    } else {
                        None
                    }
                }
            },
        }
    }

    fn meets_condition(
        &self,
        engine: &mut JsonPredicateEngine,
        trigger: &Trigger,
        context: &EventContext,
    ) -> LaikaResult<Option<serde_json::Value>> {
        engine
            .evaluate(&self.filter_and_extract, trigger, context)
            .map_err(|e| {
                error!("{}", e);
                LaikaError::RuleEvaluationError(e.to_string())
            })
    }

    pub fn evaluate(
        &self,
        engine: &mut JsonPredicateEngine,
        trigger: &Trigger,
        context: &EventContext,
    ) -> LaikaResult<RuleResult> {
        if !self.valid_correlation(trigger, context) {
            return Err(LaikaError::InvalidEventGroup);
        }
        tracing::debug!("Evaluating rule with Trigger {:?} and Context {:?}", trigger, context);
        if let Some(met_at) = self.when_met_requirements(trigger, context) {
            if let Some(condition_result) = self.meets_condition(engine, trigger, context)? {
                Ok(RuleResult::ConditionSatisfied {
                    met_at,
                    action_config: self.action.clone(),
                    condition_result,
                })
            } else {
                Ok(RuleResult::ConditionNotSatisfied {
                    met_at,
                    recheck: self.timing.clone(),
                })
            }
        } else {
            Ok(RuleResult::RequirementNotMet {})
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::builder::ActionConfig;
    use crate::config::EventRuleDefinition;
    use crate::errors::{LaikaError, LaikaResult};
    use crate::event::context::EventContext;
    use crate::event::{Event, RawEvent, Trigger};
    use crate::matcher::builder::EventMatchBuilder;
    use crate::predicate_engine::JsonPredicateEngine;
    use crate::rules::{EventRule, Requirement, RuleResult};
    use crate::template::Template;
    use serde_json::json;
    use std::collections::HashMap;

    fn event_a() -> RawEvent {
        RawEvent::new(
            json!({
                "type": "PaymentInitiated",
                "transaction_id": 1,
                "customer_id": 123,
                "value": 100.50,
                "currency": "USD"
            }), // Correlation Target should be provided by the original YAML
        )
    }

    fn event_b() -> RawEvent {
        RawEvent::new(
            json!({
                "type": "PaymentAuthorised",
                "txn_id": 1,
                "customer_id": 123
            }), // Correlation Target should be provided by the original YAML
        )
    }

    fn event_c() -> RawEvent {
        RawEvent::new(json!({
            "type": "PaymentSettled",
            "transaction_id": 1,
            "customer_id": 123
        }))
    }

    fn default_event_matcher() -> EventMatchBuilder {
        let config_str = r#"
        {
            "events": {
                "eventA": {
                    "matchKey": {
                        "$.type": "PaymentInitiated",
                    }
                },
                "eventB": {
                    "matchKey": {
                        "$.type": "PaymentAuthorised",
                    },
                },
                "eventC": {
                    "matchKey": {
                        "$.type": "PaymentSettled",
                    },
                }
            }
        }
        "#;

        let config: EventMatchBuilder = serde_json::from_str(config_str).unwrap();
        config
    }

    fn static_template() -> Template {
        let mut map = serde_yaml::mapping::Mapping::new();
        map.insert(
            serde_yaml::Value::String("Output".into()),
            serde_yaml::Value::String("Example".into()),
        );
        Template::from_payload(&serde_yaml::Value::Mapping(map)).expect("Invalid payload")
    }

    #[test]
    fn test_partial_requirements_not_satisfied() -> LaikaResult<()> {
        let mut engine = JsonPredicateEngine::default();
        let rule = EventRuleDefinition {
            name: "partialRule".to_string(),
            filter_and_extract: None,
            timing: None,
            requires: Some(Requirement::Exactly(vec![
                "eventA".to_string(),
                "eventB".to_string(),
                "eventC".to_string(),
            ])),
            action: ActionConfig {
                target: "".to_string(),
                emit_template: static_template(),
            },
        }
        .register_to_engine(&mut engine);
        let events: Vec<Event> = vec![event_a().parse("eventA", Some("a".to_string()))];
        let context: EventContext = EventContext::try_from(events)?;
        let trigger: Trigger =
            Trigger::ReceivedEvent(event_b().parse("eventB", Some("a".to_string())));

        let result = rule.evaluate(&mut engine, &trigger, &context)?;
        assert!(matches!(result, RuleResult::RequirementNotMet {}));
        Ok(())
    }

    #[test]
    fn test_raise_invalid_event_group() -> LaikaResult<()> {
        let mut engine = JsonPredicateEngine::default();
        let rule = EventRuleDefinition {
            name: "partialRule".to_string(),
            filter_and_extract: None,
            timing: None,
            requires: Some(Requirement::Exactly(vec![
                "eventA".to_string(),
                "eventB".to_string(),
            ])),
            action: ActionConfig {
                target: "".to_string(),
                emit_template: static_template(),
            },
        }
        .register_to_engine(&mut engine);

        // EventA NonCorrelated is the only item
        let events: Vec<Event> = vec![];
        let context: EventContext = EventContext::try_from(events)?;
        let trigger: Trigger = Trigger::ReceivedEvent(event_a().parse("eventA", None));
        assert!(matches!(
            rule.evaluate(&mut engine, &trigger, &context),
            Err(LaikaError::InvalidEventGroup)
        ));

        let events: Vec<Event> = vec![event_a().parse("eventA", None)];
        let context: EventContext = EventContext::try_from(events)?;
        let trigger: Trigger = Trigger::ReceivedEvent(event_b().parse("eventB", None));
        assert!(matches!(
            rule.evaluate(&mut engine, &trigger, &context),
            Err(LaikaError::InvalidEventGroup)
        ));
        Ok(())
    }

    #[test]
    // Test rule is satisfied when it has no requirements
    fn test_empty_requirements() -> LaikaResult<()> {
        let mut engine = JsonPredicateEngine::default();
        // EventRuleDefinition
        let rule = EventRuleDefinition {
            name: "partialRule".to_string(),
            filter_and_extract: None,
            timing: None,
            requires: None,
            action: ActionConfig {
                target: "".to_string(),
                emit_template: static_template(),
            },
        }
        .register_to_engine(&mut engine);

        let events: Vec<Event> = vec![
            event_a().parse("eventA", Some("a".to_string())),
            event_b().parse("eventB", Some("b".to_string())),
        ];
        let context: EventContext = EventContext::try_from(events)?;
        let trigger: Trigger =
            Trigger::ReceivedEvent(event_c().parse("eventC", Some(("c".to_string()))));

        let result = rule.evaluate(&mut engine, &trigger, &context)?;
        assert!(
            matches!(result, RuleResult::ConditionSatisfied { .. }),
            "Rule with empty requirements should always be satisfied"
        );
        Ok(())
    }
}
