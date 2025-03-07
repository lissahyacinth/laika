use crate::errors::LaikaResult;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use zmq::{Context, Socket};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct HandlerId(pub u32);

pub type CorrelationId = String;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventId(pub u64);

// serde is internal here
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
// TODO: Add a Rule to this
pub struct EventExpiry {
    pub expires_at: OffsetDateTime,
    pub correlation_id: CorrelationId,
    pub event_rule: String,
}

impl EventExpiry {
    pub fn new(
        recheck_at: OffsetDateTime,
        correlation_id: CorrelationId,
        event_rule: String,
    ) -> Self {
        Self {
            expires_at: recheck_at,
            correlation_id,
            event_rule,
        }
    }
}

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
}
