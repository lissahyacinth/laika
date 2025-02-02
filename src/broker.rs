use crate::action::EventAction;
use crate::errors::LaikaResult;
use crate::event::{Event, RawEvent};
use crate::flow::EventDefinitions;
use crate::rules::EventProcessorGroup;
use crate::storage::StorageKV;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use zmq::{Context, Socket};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct HandlerId(pub u32);

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CorrelationId(pub String);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventId(pub u64);

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventExpiry(pub OffsetDateTime, pub CorrelationId);

pub struct Broker {
    context: Context,
    socket: Socket,
    endpoint: String,
}

impl Broker {
    pub fn new(endpoint: String) -> LaikaResult<Self> {
        let context = Context::new();
        let socket = context.socket(zmq::REP)?;
        Ok(Broker {
            context,
            socket,
            endpoint,
        })
    }

    fn handle_event(
        event_definitions: &EventDefinitions,
        rule_groups: &[EventProcessorGroup],
        storage_kv: &mut StorageKV,
        raw_event: RawEvent,
    ) -> LaikaResult<Vec<EventAction>> {
        let mut event_actions: Vec<EventAction> = Vec::new();
        if let Some(event) = event_definitions.parse_event(raw_event) {
            // Start a transaction to write the event to the database for the correlation id.
            // Retrieve events from the database for the correlation id.
            // This will block other writers until this is finished.
            match event? {
                Event::Correlated(correlated_event) => {
                    let transaction = storage_kv.start_transaction();
                    let events: Vec<Event> = storage_kv
                        .write_event(&transaction, correlated_event)?
                        .into_iter()
                        .map(Event::Correlated)
                        .collect();
                    for rule_group in rule_groups {
                        event_actions.extend(rule_group.matched_actions(&events)?);
                    }
                    transaction.commit()?;
                }
                Event::NonCorrelated(non_correlated_event) => {
                    let events = vec![Event::NonCorrelated(non_correlated_event)];
                    for rule_group in rule_groups {
                        event_actions.extend(rule_group.matched_actions(&events)?);
                    }
                }
            }
        }
        Ok(event_actions)
    }

    fn handle_timing_expiry(
        rule_groups: &[EventProcessorGroup],
        storage_kv: &mut StorageKV,
        correlation_id: String,
    ) -> LaikaResult<Vec<EventAction>> {
        let mut event_actions: Vec<EventAction> = Vec::new();
        let transaction = storage_kv.start_transaction();
        let events: Vec<Event> = storage_kv
            .read_events(&transaction, correlation_id.as_str())?
            .into_iter()
            .map(Event::Correlated)
            .collect();
        for rule_group in rule_groups {
            // We don't need to provide the timing as it might be inferred from different events
            //  we just need to wake the checker. (TODO: Do we need a waker?)
            event_actions.extend(rule_group.matched_actions(&events)?);
        }
        transaction.commit()?;
        Ok(event_actions)
    }
}
