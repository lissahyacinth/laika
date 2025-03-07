pub mod builder;

use crate::errors::{LaikaError, LaikaResult};
use crate::utils::extract_json::extract_json_field;
use regex::Regex;
use serde_json::Value;
pub type MaybeEventType = Option<String>;

pub type EventType = String;

#[derive(Clone, Default)]
pub struct EventMatcher {
    event_match_rules: Vec<(EventMatchPattern, EventType)>,
}

#[derive(Clone, Debug)]
pub enum EventMatchPattern {
    All,
    MatchRules(Vec<(String, MatchOn)>), // Match Key -> Match Rule
}

#[derive(Clone, Debug)]
enum MatchOn {
    Exactly(String),
    Regex(Regex),
}

impl EventMatcher {
    pub fn new(event_match_rules: Vec<(EventMatchPattern, EventType)>) -> Self {
        Self { event_match_rules }
    }

    /// Attempts to match a JSON message against the configured event types, returning
    /// all matching types.
    fn match_rule(value: &str, match_on: &MatchOn) -> bool {
        match match_on {
            MatchOn::Exactly(matched_item) => value == matched_item.as_str(),
            MatchOn::Regex(regex) => regex.is_match(value),
        }
    }

    pub fn match_message(&self, message: &Value) -> LaikaResult<Vec<EventType>> {
        let mut matching_event_types: Vec<EventType> = Vec::new();
        for (match_pattern, event_type) in &self.event_match_rules {
            match match_pattern {
                EventMatchPattern::All => {
                    matching_event_types.push(event_type.clone());
                }
                EventMatchPattern::MatchRules(match_rules) => {
                    if match_rules
                        .iter()
                        .map(|(field_path, match_rule)| {
                            extract_json_field(message, field_path).map(|value| {
                                match value.as_str() {
                                    Some(value) => EventMatcher::match_rule(value, match_rule),
                                    None => false,
                                }
                            })
                        })
                        .try_fold(true, |acc, x| Ok::<bool, LaikaError>(acc && x?))?
                    {
                        matching_event_types.push(event_type.clone());
                    }
                }
            }
        }
        Ok(matching_event_types)
    }
}
