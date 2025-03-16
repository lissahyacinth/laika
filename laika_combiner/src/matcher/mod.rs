pub mod builder;

use crate::errors::{LaikaError, LaikaResult};
use crate::utils::extract_json::extract_json_field;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;

pub type MaybeEventType = Option<String>;

pub type EventType = String;

#[derive(Clone, Default, Debug)]
/// Defining Event Types based on the Patterns they meet.
pub struct EventTypeDefinitions {
    type_definitions: Vec<EventTypeDefinition>,
}

impl EventTypeDefinitions {
    /// All unique connection sources used in Event Definitions
    pub(crate) fn receivers(&self) -> Vec<String> {
        self.type_definitions
            .iter()
            .map(|type_definition| type_definition.source.clone())
            .collect::<HashSet<String>>()
            .into_iter()
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct EventTypeDefinition {
    source: String, // Named Connection Source for this event type
    match_pattern: EventMatchPattern,
    event_type: EventType,
}

impl EventTypeDefinition {
    pub fn new(source: String, match_pattern: EventMatchPattern, event_type: EventType) -> Self {
        Self {
            source,
            match_pattern,
            event_type,
        }
    }
}

#[derive(Clone, Debug)]
pub enum EventMatchPattern {
    /// All Events match this
    All,
    /// Events that meet all of these rules match this
    ///
    /// (MatchKey, MatchRule)
    MatchRules(Vec<(String, MatchOn)>),
}

#[derive(Clone, Debug)]
pub enum MatchOn {
    Exactly(String),
    Regex(Regex),
}

impl EventTypeDefinitions {
    pub fn new(event_match_rules: Vec<EventTypeDefinition>) -> Self {
        Self {
            type_definitions: event_match_rules,
        }
    }

    /// Attempts to match a JSON message against the configured event types, returning
    /// all matching types.
    fn match_rule(value: &str, match_on: &MatchOn) -> bool {
        match match_on {
            MatchOn::Exactly(matched_item) => value == matched_item.as_str(),
            MatchOn::Regex(regex) => regex.is_match(value),
        }
    }

    pub fn match_message(
        &self,
        event_source: &str,
        message: &Value,
    ) -> LaikaResult<Vec<EventType>> {
        let mut matching_event_types: Vec<EventType> = Vec::new();
        for event_type_definition in &self.type_definitions {
            if event_type_definition.source == event_source {
                match &event_type_definition.match_pattern {
                    EventMatchPattern::All => {
                        matching_event_types.push(event_type_definition.event_type.clone());
                    }
                    EventMatchPattern::MatchRules(match_rules) => {
                        if match_rules
                            .iter()
                            .map(|(field_path, match_rule)| {
                                extract_json_field(message, field_path).map(|value| {
                                    match value.as_str() {
                                        Some(value) => {
                                            EventTypeDefinitions::match_rule(value, match_rule)
                                        }
                                        None => false,
                                    }
                                })
                            })
                            .try_fold(true, |acc, x| Ok::<bool, LaikaError>(acc && x?))?
                        {
                            matching_event_types.push(event_type_definition.event_type.clone());
                        }
                    }
                }
            }
        }
        Ok(matching_event_types)
    }
}
