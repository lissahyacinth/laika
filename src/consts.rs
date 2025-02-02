pub(crate) const HEARTBEAT_LIVENESS: usize = 3;
pub(crate) const HEARTBEAT_INTERVAL: usize = 2500;

pub(crate) const HEARTBEAT_EXPIRY: usize = HEARTBEAT_LIVENESS * HEARTBEAT_INTERVAL;
