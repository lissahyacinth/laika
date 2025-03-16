use crate::errors::{LaikaError, LaikaResult};
use crate::event::{Event, EventLike};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;

#[derive(Clone)]
/// The content around a given event trigger, *not* including the trigger.  
///
/// Events provided in sequence are ordered upon EventContext creation.
#[derive(Debug)]
pub struct EventContext {
    sequence: Vec<Event>,
    events: HashMap<String, Vec<Event>>, // EventType -> Events
}

impl EventContext {
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.sequence.iter()
    }
}

impl TryFrom<Vec<Event>> for EventContext {
    type Error = LaikaError;

    fn try_from(value: Vec<Event>) -> LaikaResult<Self> {
        let mut sequence = value;
        sequence.sort();
        // Cannot presume pre-sorted.
        let mut events: HashMap<String, Vec<Event>> = HashMap::new();
        for event in sequence.clone().into_iter() {
            match event.event_type() {
                Some(event_type) => events.entry(event_type).or_default().push(event),
                None => {
                    return Err(LaikaError::Generic(
                        "EventContexts can only be built from events with types".to_string(),
                    ))
                }
            }
        }
        Ok(Self { sequence, events })
    }
}

impl Serialize for EventContext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Convert sequence from tuples to objects
        let sequence = self
            .sequence
            .iter()
            .map(|event| {
                json!({
                    "type": event.event_type(),
                    "data": event.get_data()
                })
            })
            .collect::<Vec<_>>();

        // Events map is already in the right structure
        json!({
            "sequence": sequence,
            "events": self.events
        })
        .serialize(serializer)
    }
}
