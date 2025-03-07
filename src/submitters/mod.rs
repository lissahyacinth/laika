use crate::submitters::rabbitmq::RabbitMQSubmitter;
use crate::submitters::stdout::StdoutSubmitter;
use async_trait::async_trait;
use serde::Deserialize;
use thiserror::Error;

mod rabbitmq;
mod stdout;

#[derive(Error, Debug)]
pub enum SubmitterError {
    #[error("Failed to connect to queue: {0}")]
    ConnectionError(String),
    #[error("Failed to create channel: {0}")]
    ChannelError(String),
    #[error("Invalid configuration: {0}")]
    ConfigError(String),
    #[error("Submission failed: {0}")]
    SubmissionError(String),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum SubmitterConfig {
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct RoutingConfig {
    topic: String,
}

#[async_trait]
pub trait EventSubmitter: Send + Sync {
    async fn submit(
        &self,
        payload: serde_json::Value,
        routing: &RoutingConfig,
    ) -> Result<(), SubmitterError>;
}

pub async fn create_submitter(
    config: SubmitterConfig,
) -> Result<Box<dyn EventSubmitter>, SubmitterError> {
    match config {
        SubmitterConfig::RabbitMQ {
            host,
            port,
            username,
            password,
            vhost,
        } => {
            let submitter = RabbitMQSubmitter::new(host, port, username, password, vhost).await?;
            Ok(Box::new(submitter))
        }
        SubmitterConfig::Stdout { .. } => Ok(Box::new(StdoutSubmitter::new()?)),
    }
}
