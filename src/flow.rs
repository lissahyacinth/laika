use crate::broker::CorrelationId;
use crate::errors::{LaikaError, LaikaResult};
use crate::event::{CorrelatedEvent, Event, EventLike, RawEvent};
use std::collections::HashMap;
use tracing::debug;

#[derive(Clone)]
pub struct EventMatcher {
    pub event_name: String,
    pub event_type: String,
}

#[derive(Clone)]
pub struct EventTypes {
    events: Vec<EventMatcher>,
}

impl EventTypes {
    pub fn matches(&self, event: &RawEvent) -> LaikaResult<Option<String>> {
        for matcher in &self.events {
            if event.event_type()? == matcher.event_type {
                return Ok(Some(matcher.event_name.clone()));
            }
        }
        Ok(None)
    }
}

pub struct EventDefinitions {
    event_types: EventTypes,
    event_correlation: HashMap<String, String>, // eventName -> jsonPath
}

impl EventDefinitions {
    pub fn parse_event(&self, raw_event: RawEvent) -> Option<LaikaResult<Event>> {
        if let Ok(Some(event_type)) = self.event_types.matches(&raw_event) {
            if let Some(correlation_path) = self.event_correlation.get(&event_type) {
                let correlation_id = raw_event
                    .try_extract(correlation_path.as_str())?
                    .to_string();
                Some(raw_event.with_correlation_id(CorrelationId(correlation_id)))
            } else {
                Some(Err(LaikaError::Generic(format!(
                    "No correlation ID found for {}",
                    event_type
                ))))
            }
        } else {
            debug!("Event was not matched {:?}", raw_event);
            None
        }
    }
}
