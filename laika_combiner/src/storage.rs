use crate::errors::LaikaResult;
use crate::event::event_serde::CorrelatedEventCapnpBatch;
use crate::event::CorrelatedEvent;
use rocksdb::{IteratorMode, OptimisticTransactionDB, Options, Transaction};
use std::path::{Path, PathBuf};

pub struct StorageKV {
    events_by_correlation_id: OptimisticTransactionDB,
}

pub struct StorageKVBuilder {
    max_total_wal_size: Option<u64>,
    parallelism: Option<usize>,
    max_background_jobs: Option<usize>,
    base_path: PathBuf,
}

impl StorageKVBuilder {
    pub fn new<P: AsRef<Path>>(base_path: P) -> StorageKVBuilder {
        StorageKVBuilder {
            max_total_wal_size: None,
            parallelism: None,
            max_background_jobs: None,
            base_path: PathBuf::from(base_path.as_ref()),
        }
    }

    pub fn max_total_wal_size(mut self, size: u64) -> StorageKVBuilder {
        self.max_total_wal_size = Some(size);
        self
    }

    pub fn parallelism(mut self, parallelism: usize) -> StorageKVBuilder {
        self.parallelism = Some(parallelism);
        self
    }
    pub fn max_background_jobs(mut self, jobs: usize) -> StorageKVBuilder {
        self.max_background_jobs = Some(jobs);
        self
    }

    pub fn build(self) -> Result<StorageKV, rocksdb::Error> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        if let Some(max_total_wal_size) = self.max_total_wal_size {
            opts.set_max_total_wal_size(max_total_wal_size);
        } else {
        }
        opts.set_max_total_wal_size(10 * 1024 * 1024 * 1024);
        opts.set_max_background_jobs(4);
        opts.increase_parallelism(4);

        StorageKV::new(self.base_path, opts)
    }
}

impl StorageKV {
    pub fn new<P: AsRef<Path>>(base_path: P, opts: Options) -> Result<Self, rocksdb::Error> {
        Ok(Self {
            events_by_correlation_id: OptimisticTransactionDB::open(
                &opts,
                base_path.as_ref().join("events_by_correlation_id"),
            )?,
        })
    }

    /// Remove all entries
    pub fn delete_all_keys(&self) -> Result<(), rocksdb::Error> {
        let keys: Vec<Vec<u8>> = self
            .events_by_correlation_id
            .iterator(IteratorMode::Start)
            .map(|item| item.unwrap().0.to_vec())
            .collect();
        for key in keys {
            self.events_by_correlation_id.delete(key)?;
        }
        self.events_by_correlation_id
            .compact_range(None::<&[u8]>, None::<&[u8]>);
        self.events_by_correlation_id.flush()?;
        tracing::debug!("All keys have been removed from KV");
        Ok(())
    }

    pub fn start_transaction(&self) -> Transaction<OptimisticTransactionDB> {
        self.events_by_correlation_id.transaction()
    }

    pub fn read_events(
        &self,
        txn: &Transaction<OptimisticTransactionDB>,
        correlation_id: &str,
    ) -> LaikaResult<Vec<CorrelatedEvent>> {
        match txn.get(correlation_id)? {
            None => Ok(Vec::new()),
            Some(events) => CorrelatedEventCapnpBatch::from_bytes(events.as_slice())?.try_into(),
        }
    }

    pub fn write_event(
        &self,
        txn: &Transaction<OptimisticTransactionDB>,
        event: CorrelatedEvent,
    ) -> LaikaResult<Vec<CorrelatedEvent>> {
        tracing::debug!("Writing Correlated Event to KV");
        let correlation_id = event.correlation_id.clone();
        let existing_events = txn.get(correlation_id.as_str())?;
        let updated_events = match existing_events {
            Some(existing) => {
                let mut existing_event_batch: CorrelatedEventCapnpBatch =
                    CorrelatedEventCapnpBatch::from_bytes(existing.as_slice())?;
                existing_event_batch.push_event(event)?;
                existing_event_batch
            }
            None => CorrelatedEventCapnpBatch::try_from(vec![event])?,
        };
        txn.put(
            correlation_id.as_str(),
            updated_events.to_bytes()?,
        )?;
        tracing::debug!("Wrote new event to KV");
        Ok(Vec::try_from(updated_events)?)
    }
}
