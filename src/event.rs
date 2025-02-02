use crate::broker::CorrelationId;
use crate::errors::{LaikaError, LaikaResult};
use crate::utils::extract_json::extract_json_field;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

pub(crate) trait EventLike {
    fn get_data(&self) -> &Value;

    fn try_extract(&self, path: &str) -> Option<Value> {
        match extract_json_field(self.get_data(), path) {
            Ok(val) => Some(val.clone()),
            Err(_) => None,
        }
    }

    fn event_type(&self) -> LaikaResult<String> {
        if let Value::String(event_type) = extract_json_field(self.get_data(), "type")? {
            Ok(event_type.to_string())
        } else {
            Err(LaikaError::Generic(
                "Event Type was not a string".to_string(),
            ))
        }
    }
}

impl EventLike for Event {
    fn get_data(&self) -> &Value {
        match self {
            Event::Correlated(e) => e.get_data(),
            Event::NonCorrelated(e) => e.get_data(),
        }
    }
}

impl EventLike for CorrelatedEvent {
    fn get_data(&self) -> &Value {
        &self.data
    }
}

impl EventLike for RawEvent {
    fn get_data(&self) -> &Value {
        &self.data
    }
}

impl EventLike for NonCorrelatedEvent {
    fn get_data(&self) -> &Value {
        &self.data
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RawEvent {
    received: OffsetDateTime,
    data: Value,
}

impl RawEvent {
    pub fn new(data: Value) -> RawEvent {
        RawEvent {
            received: OffsetDateTime::now_utc(),
            data,
        }
    }

    pub fn with_correlation_target(self, target: &str) -> LaikaResult<Event> {
        let correlation_id = self
            .try_extract(target)
            .ok_or(LaikaError::MissingCorrelationKey)?
            .to_string();
        self.with_correlation_id(CorrelationId(correlation_id))
    }

    /// Generate an Event that cannot be correlated with other events
    pub fn without_correlation_id(self) -> LaikaResult<Event> {
        Ok(Event::NonCorrelated(NonCorrelatedEvent {
            received: self.received,
            data: self.data,
        }))
    }

    pub fn with_correlation_id(self, correlation_id: CorrelationId) -> LaikaResult<Event> {
        Ok(Event::Correlated(CorrelatedEvent {
            received: self.received,
            correlation_id,
            data: self.data,
        }))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Event {
    Correlated(CorrelatedEvent),
    NonCorrelated(NonCorrelatedEvent),
}

impl Event {
    pub fn received(&self) -> &OffsetDateTime {
        match self {
            Event::Correlated(ref e) => &e.received,
            Event::NonCorrelated(ref e) => &e.received,
        }
    }

    pub(crate) fn set_received(&mut self, received: OffsetDateTime) {
        match self {
            Event::Correlated(ref mut e) => e.received = received,
            Event::NonCorrelated(ref mut e) => e.received = received,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CorrelatedEvent {
    pub(crate) received: OffsetDateTime,
    pub(crate) correlation_id: CorrelationId,
    data: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NonCorrelatedEvent {
    pub(crate) received: OffsetDateTime,
    data: Value,
}
