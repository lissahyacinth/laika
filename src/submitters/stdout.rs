use crate::submitters::{EventSubmitter, RoutingConfig, SubmitterError};
use async_trait::async_trait;

pub struct StdoutSubmitter {}

impl StdoutSubmitter {
    pub fn new() -> Result<Self, SubmitterError> {
        Ok(Self {})
    }
}

#[async_trait]
impl EventSubmitter for StdoutSubmitter {
    async fn submit(
        &self,
        payload: serde_json::Value,
        _: &RoutingConfig,
    ) -> Result<(), SubmitterError> {
        println!("{:?}", payload);
        Ok(())
    }
}
