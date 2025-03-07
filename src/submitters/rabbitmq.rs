use crate::submitters::{EventSubmitter, RoutingConfig, SubmitterError};
use async_trait::async_trait;
use lapin::{options::BasicPublishOptions, BasicProperties, Connection, ConnectionProperties};

pub struct RabbitMQSubmitter {
    channel: lapin::Channel,
}

impl RabbitMQSubmitter {
    pub async fn new(
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
        vhost: Option<String>,
    ) -> Result<Self, SubmitterError> {
        let amqp_url = format!(
            "amqp://{}:{}@{}:{}{}",
            username.unwrap_or_else(|| "guest".to_string()),
            password.unwrap_or_else(|| "guest".to_string()),
            host,
            port,
            vhost.unwrap_or_else(|| "/".to_string()),
        );

        let conn = Connection::connect(&amqp_url, ConnectionProperties::default())
            .await
            .map_err(|e| SubmitterError::ConnectionError(e.to_string()))?;

        let channel = conn
            .create_channel()
            .await
            .map_err(|e| SubmitterError::ChannelError(e.to_string()))?;

        Ok(Self { channel })
    }
}

#[async_trait]
impl EventSubmitter for RabbitMQSubmitter {
    async fn submit(
        &self,
        payload: serde_json::Value,
        routing: &RoutingConfig,
    ) -> Result<(), SubmitterError> {
        let payload = serde_json::to_vec(&payload)
            .map_err(|e| SubmitterError::SubmissionError(e.to_string()))?;

        self.channel
            .basic_publish(
                "", // default exchange
                &routing.topic,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default(),
            )
            .await
            .map_err(|e| SubmitterError::SubmissionError(e.to_string()))?;

        Ok(())
    }
}
