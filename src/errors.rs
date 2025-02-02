use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type LaikaResult<T> = Result<T, LaikaError>;

#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LaikaError {
    #[error("Generic: {0}")]
    Generic(String),

    #[error("The graph contains a cycle and is not a DAG.")]
    GraphCycleError,

    #[error("IOError: {0}")]
    IO(String),

    #[error("Missing Event in Definition: {0}")]
    MissingEvent(String),

    #[error("Missing Input: {0}")]
    MissingInput(String),

    #[error("Event did not match ")]
    EventMatchError,

    #[error("Field {0} not found in data at path {1}")]
    FieldNotFound(String, String),

    #[error("Invalid Input")]
    InvalidInput,

    #[error("Correlation key not provided and not available in payload")]
    MissingCorrelationKey,

    #[error("{0}")]
    ChannelError(String),

    #[error("Missing Task: {0}")]
    MissingTask(String),

    #[error("Events and Tasks share names - unclear which to select")]
    UnclearEventName,

    #[error("More than 1 NonCorrelatedEvent was submitted in an event batch")]
    InvalidEventGroup,
}

#[macro_export]
macro_rules! laika_bail {
    ($err:ident, $msg:literal $(,)?) => {
        return Err($crate::LaikaError::$err($msg.to_owned()))
    };
    ($err:ident, $fmt:expr, $($arg:tt)*) => {
        return Err($crate::LaikaError::$err(format!($fmt, $($arg)*)))
    };
}

macro_rules! laika_error_from {
    ($err:ty, $laika_err:ident, $func:expr) => {
        impl From<$err> for LaikaError {
            fn from(value: $err) -> Self {
                LaikaError::$laika_err($func(value))
            }
        }
    };
    ($err:ty, $laika_err:ident) => {
        impl From<$err> for LaikaError {
            fn from(value: $err) -> Self {
                LaikaError::$laika_err(value.to_string())
            }
        }
    };
}

laika_error_from!(rocksdb::Error, Generic);
laika_error_from!(bincode::Error, Generic);
laika_error_from!(zmq::Error, Generic);
