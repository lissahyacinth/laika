use crate::connections::{EventSubmitter, MessagingError};
use async_trait::async_trait;
use lapin::{Connection, ConnectionProperties};

pub struct RabbitMqConnection {
    channel: lapin::Channel,
}

impl RabbitMqConnection {
    pub async fn new(
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
        vhost: Option<String>,
    ) -> Result<Self, MessagingError> {
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
            .map_err(|e| MessagingError::ConnectionError(e.to_string()))?;

        let channel = conn
            .create_channel()
            .await
            .map_err(|e| MessagingError::ChannelError(e.to_string()))?;

        Ok(Self { channel })
    }
}

#[async_trait]
impl EventSubmitter for RabbitMqConnection {
    async fn submit(&self, _payload: serde_json::Value) -> Result<(), MessagingError> {
        todo!()
    }
}
