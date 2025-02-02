use time::OffsetDateTime;

#[derive(Clone)]
pub struct AlertAction {}

#[derive(Clone)]
pub struct EmitAction {}

#[derive(Clone)]
pub struct DelayedCheck {
    until: OffsetDateTime,
}

#[derive(Clone)]
pub enum EventAction {
    Alert(AlertAction),
    Emit(EmitAction),
    DelayedCheck(DelayedCheck),
    NoAction,
}
