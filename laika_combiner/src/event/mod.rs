pub(crate) mod context;
pub(crate) mod event_serde;

use crate::broker::{CorrelationId, EventExpiry};
use crate::matcher::MaybeEventType;
use crate::utils::extract_json::extract_json_field;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cmp::Ordering;
use time::OffsetDateTime;
pub(crate) trait EventLike {
    fn get_data(&self) -> &Value;

    fn try_extract(&self, path: &str) -> Option<Value> {
        match extract_json_field(self.get_data(), path) {
            Ok(val) => Some(val.clone()),
            Err(_) => None,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
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

    pub fn parse<S: Into<String>>(
        self,
        event_type: S,
        correlation_id: Option<CorrelationId>,
    ) -> Event {
        if let Some(correlation_id) = correlation_id {
            Event::Correlated(CorrelatedEvent {
                received: self.received,
                correlation_id,
                event_type: event_type.into(),
                data: self.data,
            })
        } else {
            Event::NonCorrelated(NonCorrelatedEvent {
                received: self.received,
                event_id: uuid::Uuid::new_v4().to_string(),
                event_type: event_type.into(),
                data: self.data,
            })
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Correlated(CorrelatedEvent),
    NonCorrelated(NonCorrelatedEvent),
}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Event {
    fn cmp(&self, other: &Self) -> Ordering {
        self.received().cmp(other.received())
    }
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

    pub fn event_type(&self) -> MaybeEventType {
        match self {
            Event::Correlated(e) => Some(e.event_type.clone()),
            Event::NonCorrelated(e) => Some(e.event_type.clone()),
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct CorrelatedEvent {
    pub(crate) received: OffsetDateTime,
    pub(crate) correlation_id: CorrelationId,
    pub(crate) event_type: String,
    pub(crate) data: Value,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct NonCorrelatedEvent {
    pub(crate) received: OffsetDateTime,
    // Non-correlated ID used to identify this Event throughout processing
    pub(crate) event_id: String,
    pub(crate) event_type: String,
    pub(crate) data: Value,
}

pub enum Trigger {
    ReceivedEvent(Event),
    TimerExpired(EventExpiry),
}

impl Serialize for Trigger {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Trigger::ReceivedEvent(event) => match event {
                Event::Correlated(correlated_event) => {
                    json!({
                        "type": "received_event",
                        "timestamp": correlated_event.received.unix_timestamp(),
                        "event": correlated_event.data
                    })
                }
                Event::NonCorrelated(uncorrelated_event) => {
                    json!({
                        "type": "received_event",
                        "timestamp": uncorrelated_event.received.unix_timestamp(),
                        "event": uncorrelated_event.data
                    })
                }
            },
            Trigger::TimerExpired(expired_event) => {
                json!({
                    "type": "timer_expired",
                    "timestamp": expired_event.expires_at.unix_timestamp(),
                })
            }
        }
        .serialize(serializer)
    }
}
