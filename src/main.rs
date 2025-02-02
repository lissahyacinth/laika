use crate::errors::LaikaResult;
use action::EventAction;

mod action;
mod broker;
mod consts;
mod errors;
mod event;
mod flow;
mod flow_definition;
mod parser;
mod rules;
mod rules_engine;
mod storage;
mod timing;
mod utils;
// Building out a CQRS pattern effectively.
// The full architecture here will be
// [Subscribers] => [Broker] => [Receivers]
// Where this component is purely the broker, and will receive data over ZeroMQ.

async fn handle_actions(targets: Vec<String>, actions: Vec<EventAction>) -> LaikaResult<()> {
    for action in actions {
        match action {
            EventAction::Alert(_) => {}
            EventAction::Emit(target) => {
                // Write to the target.
                // Initially write to a local file as a simple outbox pattern.
            }
            EventAction::DelayedCheck(_) => {}
            _ => {}
        }
    }
    Ok(())
}

fn main() {
    // Read new available event
}
