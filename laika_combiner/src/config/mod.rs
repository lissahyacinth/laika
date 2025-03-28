pub mod builder;

use crate::broker::CorrelationId;
use crate::connections::{
    create_receiver, create_submitter, ConnectionConfig, Connections, EventReceiver,
    EventSubmitter, MessagingError,
};
use crate::errors::{LaikaError, LaikaResult};
use crate::event::{EventLike, RawEvent};
use crate::matcher::{EventType, EventTypeDefinitions};
use crate::predicate_engine::{JsonPredicate, JsonPredicateEngine};
use crate::rules::{EventRule, Requirement};
use crate::EventProcessor;
use builder::{ActionConfig, TimingConfig};
use futures::stream::{self, StreamExt};
use std::collections::{HashMap, HashSet};

const DEFAULT_PREDICATE: &str = r#"(trigger, ctx) => {
  const result = {
    trigger: {
      type: trigger.type,
      timestamp: trigger.timestamp
    },
    events: {},
    meta: {}
  };

  // Add event to trigger if it's a received_event type
  if (trigger.type === "received_event" && trigger.event) {
    result.trigger.event = trigger.event;
  }

  // Process events from context
  if (ctx.events) {
    // Copy events from context to result
    for (const [eventType, eventArray] of Object.entries(ctx.events)) {
      // Only include non-empty event arrays
      if (eventArray && eventArray.length > 0) {
        result.events[eventType] = [...eventArray]; // Create a copy of the array
        result.meta[`${eventType}_count`] = eventArray.length;
      }
    }
  }

  // Determine if we should return the result or null based on your business logic
  const hasEvents = Object.keys(result.events).length > 0;

  return hasEvents ? result : null;
}"#;

#[derive(Clone, Debug)]
pub struct EventCorrelation {
    event_rules: HashMap<EventType, String>,
}

impl EventCorrelation {
    pub fn new(event_rules: HashMap<EventType, String>) -> Self {
        Self { event_rules }
    }

    pub fn correlation_id(
        &self,
        event_type: &EventType,
        event: &RawEvent,
    ) -> LaikaResult<Option<CorrelationId>> {
        if let Some(correlation_path) = self.event_rules.get(event_type) {
            Ok(Some(
                event
                    .try_extract(correlation_path.as_str())
                    .ok_or(LaikaError::EventMatchError)?
                    .to_string(),
            ))
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Debug)]
pub struct EventTrigger {
    requirement: Requirement,
    filter_and_extract: Option<String>, // JS Compatible Condition
    timing: Option<TimingConfig>,
    action: ActionConfig,
}

#[derive(Clone)]
pub enum EventMatchType {
    MatchAll,
    MatchKey(HashMap<String, MatchPattern>),
}

#[derive(Clone)]
pub enum MatchPattern {
    Exact(String),
    Regex(String),
}

#[derive(Clone)]
pub struct EventRuleDefinition {
    pub(crate) name: String,
    pub(crate) filter_and_extract: Option<String>,
    pub(crate) timing: Option<TimingConfig>,
    pub(crate) requires: Option<Requirement>,
    pub(crate) action: ActionConfig,
}

impl EventRuleDefinition {
    pub fn register_to_engine(self, engine: &mut JsonPredicateEngine) -> EventRule {
        let predicate: JsonPredicate = if let Some(ref provided_condition) = self.filter_and_extract
        {
            engine.store_predicate(&provided_condition)
        } else {
            engine.store_predicate(DEFAULT_PREDICATE)
        };
        EventRule {
            name: self.name,
            filter_and_extract: predicate,
            timing: self.timing,
            requires: self.requires,
            action: self.action,
        }
    }
}

pub struct EventProcessorConfigBuilder {
    connections: HashMap<String, ConnectionConfig>,
    correlation: Option<EventCorrelation>,
    event_matcher: Option<EventTypeDefinitions>,
    triggers: Option<HashMap<EventType, EventTrigger>>,
}

impl EventProcessorConfigBuilder {
    pub fn new() -> Self {
        Self {
            connections: HashMap::default(),
            correlation: None,
            event_matcher: None,
            triggers: None,
        }
    }

    pub fn with_connections(mut self, connections: HashMap<String, ConnectionConfig>) -> Self {
        self.connections = connections;
        self
    }

    pub fn with_correlation(mut self, correlation: EventCorrelation) -> Self {
        self.correlation = Some(correlation);
        self
    }

    pub fn with_event_matcher(mut self, matcher: EventTypeDefinitions) -> Self {
        self.event_matcher = Some(matcher);
        self
    }

    pub fn with_triggers(mut self, triggers: HashMap<EventType, EventTrigger>) -> Self {
        self.triggers = Some(triggers);
        self
    }

    pub fn build(self) -> LaikaResult<EventProcessorConfig> {
        // Default event matcher if not provided
        let event_matcher = self.event_matcher.unwrap_or_default();

        // Default triggers to empty HashMap if not provided
        let triggers = self.triggers.unwrap_or_default();

        let get_connection =
            |name: &str, connection_type: &str| -> LaikaResult<(String, ConnectionConfig)> {
                let name_string = name.to_string();
                self.connections
                    .get(name)
                    .ok_or_else(|| {
                        LaikaError::Generic(format!(
                            "Connection {} listed as a {} but not provided",
                            name, connection_type
                        ))
                    })
                    .map(|config| (name_string, config.clone()))
            };

        let receiver_configs = event_matcher
            .receivers()
            .into_iter()
            .map(|source_name| get_connection(&source_name, "source"))
            .collect::<LaikaResult<HashMap<String, ConnectionConfig>>>()?;

        let target_names: HashSet<String> = triggers
            .values()
            .map(|trigger| trigger.action.target.clone())
            .collect();

        let target_configs = target_names
            .into_iter()
            .map(|target_name| get_connection(&target_name, "target"))
            .collect::<LaikaResult<HashMap<String, ConnectionConfig>>>()?;

        let correlation_rules = self
            .correlation
            .unwrap_or_else(|| EventCorrelation::new(HashMap::new()));

        Ok(EventProcessorConfig {
            receiver_configs,
            target_configs,
            correlation_rules,
            event_matcher,
            triggers,
        })
    }
}

#[derive(Clone, Debug)]
pub struct EventProcessorConfig {
    receiver_configs: HashMap<String, ConnectionConfig>,
    target_configs: HashMap<String, ConnectionConfig>,
    correlation_rules: EventCorrelation,
    event_matcher: EventTypeDefinitions,
    triggers: HashMap<EventType, EventTrigger>,
}

impl EventProcessorConfig {
    fn event_rules(&self) -> Vec<EventRuleDefinition> {
        let mut rules: Vec<EventRuleDefinition> = Vec::with_capacity(self.triggers.len());
        for (rule_name, trigger_config) in self.triggers.clone() {
            rules.push(EventRuleDefinition {
                name: rule_name,
                filter_and_extract: trigger_config.filter_and_extract,
                timing: trigger_config.timing,
                requires: if trigger_config.requirement.is_empty() {
                    None
                } else {
                    Some(trigger_config.requirement)
                },
                action: trigger_config.action,
            })
        }
        rules
    }

    async fn targets(&self) -> Result<Vec<(String, Box<dyn EventSubmitter>)>, MessagingError> {
        stream::iter(self.target_configs.clone())
            .then(|(target_name, target_config)| async move {
                create_submitter(target_config)
                    .await
                    .and_then(|submitter| Ok((target_name, submitter)))
            })
            .collect::<Vec<Result<(String, Box<dyn EventSubmitter>), MessagingError>>>()
            .await
            .into_iter()
            .collect()
    }

    async fn receivers(&self) -> Result<Vec<(String, Box<dyn EventReceiver>)>, MessagingError> {
        stream::iter(self.receiver_configs.clone())
            .then(|(receiver_name, receiver_config)| async move {
                create_receiver(receiver_config)
                    .await
                    .and_then(|submitter| Ok((receiver_name, submitter)))
            })
            .collect::<Vec<Result<(String, Box<dyn EventReceiver>), MessagingError>>>()
            .await
            .into_iter()
            .collect()
    }

    pub async fn connections(&self) -> Result<Connections, MessagingError> {
        Ok(Connections::new(
            self.receivers().await?.into_iter().collect(),
            self.targets().await?.into_iter().collect(),
        ))
    }

    pub fn build(self) -> EventProcessor {
        let rules = self.event_rules();
        EventProcessor::new(self.event_matcher, self.correlation_rules, rules)
    }
}
