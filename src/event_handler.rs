use crate::action::EventAction;
use crate::broker::EventExpiry;
use crate::errors::LaikaResult;
use crate::event::context::EventContext;
use crate::event::{CorrelatedEvent, Event, RawEvent, Trigger};
use crate::event_processor::processor::EventProcessor;
use crate::storage::StorageKV;
use tracing::span;

fn handle_correlated_parsed_event(
    processor: &mut EventProcessor,
    storage_kv: &mut StorageKV,
    correlated_event: CorrelatedEvent,
) -> LaikaResult<Vec<EventAction>> {
    let correlated_event_span = span!(tracing::Level::INFO, "handle_correlated_parsed_event");
    let _enter = correlated_event_span.enter();
    let mut event_actions: Vec<EventAction> = Vec::new();
    let correlation_id = correlated_event.correlation_id.clone();
    let transaction = storage_kv.start_transaction();
    let mut context = storage_kv
        .write_event(&transaction, correlated_event)?
        .into_iter()
        .map(Event::Correlated)
        .collect::<Vec<Event>>();
    let trigger_event = Trigger::ReceivedEvent(
        context
            .pop()
            .expect("Events will always contain the most recently triggered event"),
    );
    let context = EventContext::try_from(context)?;
    event_actions.extend(processor.relevant_actions(
        &Some(correlation_id),
        &trigger_event,
        &context,
    )?);
    transaction.commit()?;
    Ok(event_actions)
}

/// Produce required CQRS Actions for received actions.
pub fn handle_raw_event(
    processors: &mut [EventProcessor],
    storage_kv: &mut StorageKV,
    raw_event: RawEvent,
) -> LaikaResult<Vec<EventAction>> {
    let mut event_actions: Vec<EventAction> = vec![];
    for processor in processors {
        let span = tracing::span!(tracing::Level::TRACE, "Processing event against processor");
        let _enter = span.enter();
        for parsed_event in processor.parse_event(raw_event.clone())? {
            // Start a transaction to write the event to the database for the correlation id.
            // Retrieve events from the database for the correlation id.
            // This will block other writers until this is finished.
            match parsed_event {
                Event::Correlated(correlated_event) => {
                    tracing::debug!("Handling Correlated Event {:?}", &correlated_event);
                    event_actions.extend(handle_correlated_parsed_event(
                        processor,
                        storage_kv,
                        correlated_event,
                    )?);
                }
                Event::NonCorrelated(non_correlated_event) => {
                    tracing::debug!("Handling NonCorrelated Event {:?}", &non_correlated_event);
                    let trigger_event =
                        Trigger::ReceivedEvent(Event::NonCorrelated(non_correlated_event));
                    let context = EventContext::try_from(vec![])?;
                    event_actions.extend(processor.relevant_actions(
                        &None,
                        &trigger_event,
                        &context,
                    )?);
                }
            }
        }
    }
    Ok(event_actions)
}

pub fn handle_timing_expiry(
    rule_groups: &mut [EventProcessor],
    storage_kv: &mut StorageKV,
    correlation_id: String,
    event_expiry: EventExpiry,
) -> LaikaResult<Vec<EventAction>> {
    let correlation_id_str = correlation_id.clone();
    let correlation_id = Some(correlation_id.clone());
    let actions: Vec<EventAction> = vec![];
    let transaction = storage_kv.start_transaction();
    let context = EventContext::try_from(
        storage_kv
            .read_events(&transaction, correlation_id_str.as_str())?
            .into_iter()
            .map(Event::Correlated)
            .collect::<Vec<Event>>(),
    )?;
    let mut event_actions = Vec::new();
    let trigger = Trigger::TimerExpired(event_expiry);
    for rule_group in rule_groups {
        event_actions.extend(rule_group.relevant_actions(&correlation_id, &trigger, &context)?);
    }
    transaction.commit()?;
    Ok(actions)
}
