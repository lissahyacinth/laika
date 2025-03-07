use crate::broker::EventExpiry;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct DelayedCheck {
    pub until: OffsetDateTime,
}

#[derive(Clone)]
pub struct EmitAction {
    // TODO: Verify this target actually exists before allowing emitting to it.
    target: String,
    /// Rendered payload to be provided to the downstream
    payload: serde_json::Value,
}

impl EmitAction {
    pub fn new(target: String, event: serde_json::Value) -> Self {
        Self { target, payload: event }
    }

    pub fn payload(self) -> serde_json::Value {
        self.payload
    }
}

#[derive(Clone)]
pub enum EventAction {
    Emit(EmitAction),
    ScheduleWakeup(EventExpiry),
}
