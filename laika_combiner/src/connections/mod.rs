use crate::connections::file::FileEventQueue;
use crate::connections::rabbitmq::RabbitMqConnection;
use crate::connections::stdout::StdoutSubmitter;
use crate::errors::{LaikaError, LaikaResult};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use thiserror::Error;

mod file;
mod rabbitmq;
mod stdout;

#[derive(Error, Debug)]
pub enum MessagingError {
    #[error("Failed to connect to queue: {0}")]
    ConnectionError(String),
    #[error("Failed to create channel: {0}")]
    ChannelError(String),
    #[error("Invalid configuration: {0}")]
    ConfigError(String),
    #[error("Submission failed: {0}")]
    SubmissionError(String),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON Error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Stream Finished")]
    StreamFinished,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ConnectionConfig {
    #[serde(rename = "rabbitmq")]
    RabbitMQ {
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
        vhost: Option<String>,
    },
    #[serde(rename = "stdout")]
    Stdout {},
    #[serde(rename = "file")]
    File { path: String },
}

#[derive(Debug, Deserialize, Clone)]
pub struct RoutingConfig {
    topic: String,
}

#[async_trait]
pub trait EventSubmitter: Send + Sync {
    async fn submit(&self, payload: serde_json::Value) -> Result<(), MessagingError>;
}

#[async_trait]
pub trait EventReceiver: Send + Sync {
    async fn receive_one(&self) -> Result<Option<serde_json::Value>, MessagingError>;
}

pub async fn create_submitter(
    config: ConnectionConfig,
) -> Result<Box<dyn EventSubmitter>, MessagingError> {
    match config {
        ConnectionConfig::RabbitMQ {
            host,
            port,
            username,
            password,
            vhost,
        } => {
            let submitter = RabbitMqConnection::new(host, port, username, password, vhost).await?;
            Ok(Box::new(submitter))
        }
        ConnectionConfig::Stdout { .. } => Ok(Box::new(StdoutSubmitter::new()?)),
        ConnectionConfig::File { path } => Ok(Box::new(FileEventQueue::new(&*path).await?)),
    }
}

pub async fn create_receiver(
    config: ConnectionConfig,
) -> Result<Box<dyn EventReceiver>, MessagingError> {
    match config {
        ConnectionConfig::RabbitMQ {
            host,
            port,
            username,
            password,
            vhost,
        } => {
            todo!()
        }
        ConnectionConfig::Stdout { .. } => unimplemented!(), // Cannot be implemented
        ConnectionConfig::File { path } => Ok(Box::new(FileEventQueue::new(&*path).await?)),
    }
}

pub struct Connections {
    receivers: HashMap<String, Box<dyn EventReceiver>>,
    submitters: HashMap<String, Box<dyn EventSubmitter>>,
}

pub type AckCallback =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = LaikaResult<()>> + Send>> + Send + Sync>;

impl Connections {
    /// Create a Connection object from available connections, as well as named receivers and targets
    pub async fn new(
        receivers: Vec<String>,
        targets: Vec<String>,
        connections: HashMap<String, ConnectionConfig>,
    ) -> LaikaResult<Self> {
        todo!()
    }

    /// Submit a single message to a target
    pub async fn submit_to(&self, target: &str, payload: serde_json::Value) -> LaikaResult<()> {
        match self.submitters.get(target) {
            None => Err(LaikaError::Generic(format!(
                "Submitter not found for {}",
                target
            ))),
            Some(submitter) => submitter.submit(payload).await.map_err(|e| {
                LaikaError::Generic(format!("Could not submit due to {}", e.to_string()))
            }),
        }
    }

    /// Receive a batch of messages from available connections
    pub async fn receive(&self) -> LaikaResult<Vec<(serde_json::Value, AckCallback)>> {
        todo!()
    }
}
