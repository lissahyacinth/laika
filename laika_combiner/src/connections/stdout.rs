use crate::connections::{EventSubmitter, MessagingError};
use async_trait::async_trait;

#[derive(Debug)]
pub struct StdoutSubmitter {}

impl StdoutSubmitter {
    pub fn new() -> Result<Self, MessagingError> {
        Ok(Self {})
    }
}

#[async_trait]
impl EventSubmitter for StdoutSubmitter {
    async fn submit(&self, payload: serde_json::Value) -> Result<(), MessagingError> {
        println!("{:?}", payload);
        Ok(())
    }
}
