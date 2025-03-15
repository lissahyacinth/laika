use crate::errors::LaikaResult;
use action::EventAction;

pub mod action;
mod broker;
pub mod config;
pub mod connections;
pub mod errors;
pub mod event;
pub mod event_handler;
pub mod event_processor;
mod event_schema_capnp;
mod matcher;
mod predicate_engine;
mod rules;
pub mod storage;
mod template;
pub mod timing;
mod utils;

pub use event_processor::processor::EventProcessor;

// Building out a CQRS pattern effectively.
// The full architecture here will be
// [Subscribers] => [Broker] => [Receivers]
// Where this component is purely the broker, and will receive data over ZeroMQ.

async fn handle_actions(targets: Vec<String>, actions: Vec<EventAction>) -> LaikaResult<()> {
    for action in actions {
        match action {
            EventAction::Emit(target) => {
                // Write to the target.
                // Initially write to a local file as a simple outbox pattern.
            }
            EventAction::ScheduleWakeup(_) => {}
            _ => {}
        }
    }
    Ok(())
}
