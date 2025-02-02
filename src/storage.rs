use crate::errors::LaikaResult;
use crate::event::CorrelatedEvent;
use rocksdb::{OptimisticTransactionDB, Options, Transaction};
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
            Some(events) => bincode::deserialize(&events).map_err(|e| e.into()),
        }
    }

    pub fn write_event(
        &self,
        txn: &Transaction<OptimisticTransactionDB>,
        event: CorrelatedEvent,
    ) -> LaikaResult<Vec<CorrelatedEvent>> {
        let correlation_id = event.correlation_id.clone();
        let existing_events = txn.get(correlation_id.0.as_str())?;
        let updated_events = match existing_events {
            Some(existing) => {
                let mut existing_events: Vec<CorrelatedEvent> = bincode::deserialize(&existing)?;
                existing_events.push(event);
                existing_events
            }
            None => vec![event],
        };
        txn.put(
            correlation_id.0.as_str(),
            bincode::serialize(&updated_events)?,
        )?;
        Ok(updated_events)
    }
}
