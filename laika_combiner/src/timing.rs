use crate::broker::{CorrelationId, EventExpiry};
use crate::errors::{LaikaError, LaikaResult};
use fs2::FileExt;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use time::OffsetDateTime;

/// TimingExpiry tracks time windows for correlated events, enabling config like
/// "if A and B don't occur within 30 minutes, do X". Events are linked by a
/// correlation ID and persist across service restarts.
pub struct TimingExpiry {
    expiry: Option<EventExpiry>,
    source: PathBuf,
}

impl TimingExpiry {
    /// Creates a new TimingExpiry instance, loading any existing expiries from disk
    pub fn new(source: PathBuf) -> LaikaResult<Self> {
        let mut timing_expiry = Self {
            expiry: None,
            source,
        };
        timing_expiry.expiry = timing_expiry.read_expiries()?.first().cloned();
        Ok(timing_expiry)
    }

    /// Returns the next expiry to be processed without acknowledging it
    pub fn peek(&self) -> Option<EventExpiry> {
        self.expiry.clone()
    }

    /// Adds a time window to check for correlated events. When the time expires,
    /// the system can check if all expected events occurred for this correlation ID.
    pub fn add_expiry(&mut self, expiry: EventExpiry) -> LaikaResult<()> {
        let mut current = self.read_expiries()?;
        current.push(expiry);
        self.update_expiry(current)
    }

    pub fn add_expiries(&mut self, expiries: Vec<EventExpiry>) -> LaikaResult<()> {
        let mut current = self.read_expiries()?;
        current.extend(expiries);
        self.update_expiry(current)
    }

    /// Acknowledges and removes the current expiry if its time has passed.
    /// Returns error if no expiry exists or if the expiry time hasn't been reached.
    pub fn ack(&mut self) -> LaikaResult<()> {
        match self.expiry.take() {
            None => Err(LaikaError::Generic("No expiry to acknowledge".to_string())),
            Some(expiry) => {
                if expiry.expires_at > OffsetDateTime::now_utc() {
                    self.expiry = Some(expiry); // Put it back
                    Err(LaikaError::Generic("Expiry not yet met".to_string()))
                } else {
                    self.remove_expiry(expiry)?;
                    Ok(())
                }
            }
        }
    }

    /// Negatively acknowledge and remove an expiry with the given correlation ID,
    /// indicating all expected correlated events have been received and the expiry
    /// can be safely removed.
    pub fn nack(&mut self, correlation_id: CorrelationId) -> LaikaResult<()> {
        let mut expiries = self.read_expiries()?;
        let original_len = expiries.len();

        expiries.retain(|exp| exp.correlation_id != correlation_id);

        if expiries.len() == original_len {
            return Err(LaikaError::Generic(format!(
                "No expiry found for correlation ID: {}",
                correlation_id
            )));
        }

        self.update_expiry(expiries)
    }

    fn update_expiry(&mut self, mut expiries: Vec<EventExpiry>) -> LaikaResult<()> {
        expiries.sort();
        let file = File::create(&self.source).map_err(|e| LaikaError::IO(e.to_string()))?;
        file.try_lock_exclusive()
            .map_err(|e| LaikaError::IO(e.to_string()))?;
        let mut writer = BufWriter::new(&file);
        self.expiry = expiries.first().cloned();
        bincode::serialize_into(&mut writer, &expiries)
            .map_err(|e| LaikaError::IO(format!("Failed to write expiries due to {}", e)))?;
        writer.flush().map_err(|e| LaikaError::IO(e.to_string()))?;
        file.unlock().map_err(|e| LaikaError::IO(e.to_string()))?;
        Ok(())
    }

    fn remove_expiry(&mut self, expiry: EventExpiry) -> LaikaResult<()> {
        let mut expiries = self.read_expiries()?;
        expiries.retain(|exp| exp != &expiry);
        self.update_expiry(expiries)
    }

    fn read_expiries(&mut self) -> LaikaResult<Vec<EventExpiry>> {
        let file = match File::open(&self.source) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(LaikaError::IO(e.to_string())),
        };
        let metadata = file
            .metadata()
            .map_err(|e| LaikaError::IO(format!("Failed to read file metadata: {}", e)))?;
        if metadata.len() == 0 {
            return Ok(Vec::new());
        }
        let reader = BufReader::new(file);
        bincode::deserialize_from(reader)
            .map_err(|e| LaikaError::IO(format!("Failed to read expiries due to {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::CorrelationId;
    use tempfile::NamedTempFile;
    use time::OffsetDateTime;

    fn create_test_expiry() -> (TimingExpiry, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let expiry = TimingExpiry::new(temp.path().to_path_buf()).unwrap();
        (expiry, temp)
    }

    fn create_test_event(minutes_from_now: i64) -> EventExpiry {
        EventExpiry::new(
            OffsetDateTime::now_utc() + time::Duration::minutes(minutes_from_now),
            "test-id".to_string(),
            "ExampleEventRule".to_string(),
        )
    }

    #[test]
    fn test_new_expiry_is_empty() {
        let (expiry, _temp) = create_test_expiry();
        assert!(expiry.peek().is_none());
    }

    #[test]
    fn test_add_expiry() -> LaikaResult<()> {
        let (mut expiry, _temp) = create_test_expiry();
        let event = create_test_event(5);
        expiry.add_expiry(event.clone())?;

        assert_eq!(expiry.peek(), Some(event));
        Ok(())
    }

    #[test]
    fn test_multiple_expiries_ordered() -> LaikaResult<()> {
        let (mut expiry, _temp) = create_test_expiry();

        let later = create_test_event(10);
        let sooner = create_test_event(5);

        expiry.add_expiry(later.clone())?;
        expiry.add_expiry(sooner.clone())?;

        // Should return soonest expiry
        assert_eq!(expiry.peek(), Some(sooner));
        Ok(())
    }

    #[test]
    fn test_ack_future_expiry_fails() -> LaikaResult<()> {
        let (mut expiry, _temp) = create_test_expiry();
        let future_event = create_test_event(5);

        expiry.add_expiry(future_event)?;

        assert!(expiry.ack().is_err());
        Ok(())
    }

    #[test]
    fn test_ack_past_expiry_succeeds() -> LaikaResult<()> {
        let (mut expiry, _temp) = create_test_expiry();
        let past_event = create_test_event(-5);

        expiry.add_expiry(past_event.clone())?;
        expiry.ack()?;

        assert!(expiry.peek().is_none());
        Ok(())
    }

    #[test]
    fn test_expiries_persist_across_instances() -> LaikaResult<()> {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();

        let event = create_test_event(5);

        // First instance
        {
            let mut expiry = TimingExpiry::new(path.clone())?;
            expiry.add_expiry(event.clone())?;
        }

        // Second instance
        let expiry = TimingExpiry::new(path)?;
        assert_eq!(expiry.peek(), Some(event));

        Ok(())
    }

    #[test]
    fn test_remove_expiry() -> LaikaResult<()> {
        let (mut expiry, _temp) = create_test_expiry();

        let event1 = create_test_event(5);
        let event2 = create_test_event(10);

        expiry.add_expiry(event1.clone())?;
        expiry.add_expiry(event2.clone())?;

        // Remove first expiry
        expiry.remove_expiry(event1)?;

        assert_eq!(expiry.peek(), Some(event2));
        Ok(())
    }
}
