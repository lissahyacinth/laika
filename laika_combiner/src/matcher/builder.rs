use crate::config::builder::MatchOptionsBuilder;
use crate::errors::{LaikaError, LaikaResult};
use crate::matcher::{EventMatchPattern, EventTypeDefinition, EventTypeDefinitions, MatchOn};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum MatchPatternBuilder {
    Exact(String),
    Regex { regex: String },
}

impl TryFrom<MatchOptionsBuilder> for EventMatchPattern {
    type Error = LaikaError;
    fn try_from(builder: MatchOptionsBuilder) -> LaikaResult<Self> {
        if builder.match_all.is_some() {
            if builder.match_key.is_some() {
                return Err(LaikaError::Generic(
                    "Cannot specify both matchAll and matchKey".into(),
                ));
            }
            return Ok(EventMatchPattern::All);
        }
        if let Some(match_key) = builder.match_key {
            return Ok(EventMatchPattern::MatchRules(
                match_key
                    .into_iter()
                    .map(|(field, pattern)| pattern.try_into().map(|p| (field, p)))
                    .collect::<LaikaResult<Vec<(String, MatchOn)>>>()?,
            ));
        }
        // If we reach here, neither matchAll nor matchKey was specified, which is an error
        Err(LaikaError::Generic(
            "Must specify either matchAll or matchKey".into(),
        ))
    }
}

impl TryFrom<MatchPatternBuilder> for MatchOn {
    type Error = LaikaError;
    fn try_from(value: MatchPatternBuilder) -> LaikaResult<Self> {
        match value {
            MatchPatternBuilder::Exact(string) => Ok(MatchOn::Exactly(string)),
            MatchPatternBuilder::Regex { regex } => match Regex::new(&*regex) {
                Ok(re) => Ok(MatchOn::Regex(re)),
                Err(e) => Err(LaikaError::RegexError(e.to_string())),
            },
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct EventMatchBuilder(HashMap<String, MatchOptionsBuilder>);

impl EventMatchBuilder {
    pub fn new() -> Self {
        EventMatchBuilder {
            0: Default::default(),
        }
    }

    pub fn build(self) -> LaikaResult<EventTypeDefinitions> {
        let event_match_rules = self
            .0
            .into_iter()
            .map(|(event_type, match_pattern)| {
                let event_source = match_pattern.from.clone();
                EventMatchPattern::try_from(match_pattern).map(|mp| {
                    EventTypeDefinition::new(event_source, mp, event_type)
                })
            })
            .collect::<LaikaResult<Vec<EventTypeDefinition>>>()?;
        Ok(EventTypeDefinitions { event_match_rules })
    }
}
