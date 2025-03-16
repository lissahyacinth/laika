use crate::errors::{LaikaError, LaikaResult};
use crate::event::{CorrelatedEvent, NonCorrelatedEvent};
use crate::event_schema_capnp;
use crate::event_schema_capnp::{correlated_event, correlated_event_batch, non_correlated_event};
use capnp::message::{Builder, HeapAllocator};
use time::OffsetDateTime;

pub struct NonCorrelatedEventCapnp {
    pub received: i64, // Timestamp in ms since epoch
    pub event_id: String,
    pub event_type: String,
    pub data: Vec<u8>, // Serialized JSON
}

#[derive(Clone)]
pub struct CorrelatedEventCapnp {
    pub received: i64, // Timestamp in ms since epoch
    pub correlation_id: String,
    pub event_type: String,
    pub data: Vec<u8>, // Serialized JSON
}

#[derive(Clone)]
pub struct CorrelatedEventCapnpBatch {
    events: Vec<CorrelatedEventCapnp>,
}

pub struct NonCorrelatedEventCapnpBatch {
    events: Vec<NonCorrelatedEventCapnp>,
}

// Converting to intermediary format
impl TryFrom<Vec<NonCorrelatedEvent>> for NonCorrelatedEventCapnpBatch {
    type Error = LaikaError;

    fn try_from(value: Vec<NonCorrelatedEvent>) -> Result<Self, Self::Error> {
        let events: Vec<NonCorrelatedEventCapnp> = value
            .into_iter()
            .map(|v| NonCorrelatedEventCapnp::try_from(v))
            .collect::<LaikaResult<Vec<NonCorrelatedEventCapnp>>>()?;
        Ok(NonCorrelatedEventCapnpBatch { events })
    }
}

impl TryFrom<NonCorrelatedEvent> for NonCorrelatedEventCapnp {
    type Error = LaikaError;
    fn try_from(value: NonCorrelatedEvent) -> Result<Self, Self::Error> {
        Ok(NonCorrelatedEventCapnp {
            received: value.received.unix_timestamp(),
            event_id: value.event_id,
            event_type: value.event_type,
            data: serde_json::to_vec(&value.data).map_err(|e| LaikaError::IO(e.to_string()))?,
        })
    }
}

impl TryFrom<CorrelatedEventCapnpBatch> for Vec<CorrelatedEvent> {
    type Error = LaikaError;

    fn try_from(value: CorrelatedEventCapnpBatch) -> Result<Self, Self::Error> {
        Ok(value
            .events
            .into_iter()
            .map(|e| e.try_into())
            .collect::<LaikaResult<Vec<CorrelatedEvent>>>()?)
    }
}

impl TryFrom<Vec<CorrelatedEvent>> for CorrelatedEventCapnpBatch {
    type Error = LaikaError;

    fn try_from(value: Vec<CorrelatedEvent>) -> Result<Self, Self::Error> {
        let events: Vec<CorrelatedEventCapnp> = value
            .into_iter()
            .map(|v| CorrelatedEventCapnp::try_from(v))
            .collect::<LaikaResult<Vec<CorrelatedEventCapnp>>>()?;
        Ok(CorrelatedEventCapnpBatch { events })
    }
}

impl TryFrom<CorrelatedEvent> for CorrelatedEventCapnp {
    type Error = LaikaError;
    fn try_from(value: CorrelatedEvent) -> Result<Self, Self::Error> {
        Ok(CorrelatedEventCapnp {
            received: value.received.unix_timestamp(),
            correlation_id: value.correlation_id,
            event_type: value.event_type,
            data: serde_json::to_vec(&value.data).map_err(|e| LaikaError::IO(e.to_string()))?,
        })
    }
}

impl TryInto<CorrelatedEvent> for CorrelatedEventCapnp {
    type Error = LaikaError;

    fn try_into(self) -> Result<CorrelatedEvent, Self::Error> {
        Ok(CorrelatedEvent {
            received: OffsetDateTime::from_unix_timestamp(self.received).unwrap(),
            correlation_id: self.correlation_id,
            event_type: self.event_type,
            data: serde_yaml::from_slice(&self.data).map_err(|e| LaikaError::IO(e.to_string()))?,
        })
    }
}

impl TryFrom<NonCorrelatedEventCapnp> for NonCorrelatedEvent {
    type Error = LaikaError;

    fn try_from(value: NonCorrelatedEventCapnp) -> Result<Self, Self::Error> {
        Ok(NonCorrelatedEvent {
            received: OffsetDateTime::from_unix_timestamp(value.received).unwrap(),
            event_id: value.event_id,
            event_type: value.event_type,
            data: serde_yaml::from_slice(&value.data).map_err(|e| LaikaError::IO(e.to_string()))?,
        })
    }
}

// Reading/Writing to Capnp
impl NonCorrelatedEventCapnp {
    pub fn write_capnp(&self, mut event: non_correlated_event::Builder) {
        event.set_received(self.received);
        event.set_event_id(&self.event_id);
        event.set_event_type(&self.event_type);
        event.set_data(&self.data);
    }

    pub fn read_capnp(reader: non_correlated_event::Reader) -> LaikaResult<Self> {
        let received = reader.get_received();
        let event_id = reader.get_event_id().unwrap().to_string().unwrap();
        let event_type = reader.get_event_type().unwrap().to_string().unwrap();
        let data = reader.get_data().unwrap().to_vec();

        Ok(Self {
            received,
            event_id,
            event_type,
            data,
        })
    }
}

impl CorrelatedEventCapnp {
    pub fn write_capnp(&self, mut event: correlated_event::Builder) {
        event.set_received(self.received);
        event.set_correlation_id(&self.correlation_id);
        event.set_event_type(&self.event_type);
        event.set_data(&self.data);
    }

    pub fn read_capnp(reader: correlated_event::Reader) -> LaikaResult<Self> {
        let received = reader.get_received();
        let correlation_id = reader.get_correlation_id().unwrap().to_string().unwrap();
        let event_type = reader.get_event_type().unwrap().to_string().unwrap();
        let data = reader.get_data().unwrap().to_vec();

        Ok(Self {
            received,
            correlation_id,
            event_type,
            data,
        })
    }
}

impl CorrelatedEventCapnpBatch {
    pub fn push_event<T: TryInto<CorrelatedEventCapnp>>(&mut self, event: T) -> LaikaResult<()>
    where
        LaikaError: From<<T as TryInto<CorrelatedEventCapnp>>::Error>,
    {
        let event: CorrelatedEventCapnp = event.try_into()?;
        self.events.push(event);
        Ok(())
    }

    pub fn from_bytes(bytes: &[u8]) -> LaikaResult<Self> {
        // Create a message reader from the bytes
        let message_reader =
            capnp::serialize::read_message(bytes, capnp::message::ReaderOptions::default())?;
        let batch_reader =
            message_reader.get_root::<event_schema_capnp::correlated_event_batch::Reader>()?;
        Self::read_capnp(batch_reader)
    }

    pub fn to_bytes(&self) -> LaikaResult<Vec<u8>> {
        let mut message = Builder::new_default();
        self.write_capnp(&mut message)?;
        let mut output = Vec::new();
        capnp::serialize::write_message(&mut output, &message)?;
        Ok(output)
    }

    fn write_capnp(&self, builder: &mut Builder<HeapAllocator>) -> LaikaResult<()> {
        let event_batch = builder.init_root::<correlated_event_batch::Builder>();
        let mut events_list = event_batch.init_events(self.events.len() as u32);

        for (i, event) in self.events.iter().enumerate() {
            let mut event_builder = events_list.reborrow().get(i as u32);
            event_builder.set_received(event.received);
            event_builder.set_correlation_id(&event.correlation_id);
            event_builder.set_event_type(&event.event_type);
            event_builder.set_data(&event.data);
        }

        Ok(())
    }

    fn read_capnp(reader: event_schema_capnp::correlated_event_batch::Reader) -> LaikaResult<Self> {
        let events_list = reader.get_events()?;
        let mut events = Vec::with_capacity(events_list.len() as usize);

        for i in 0..events_list.len() {
            events.push(CorrelatedEventCapnp::read_capnp(events_list.get(i))?);
        }

        Ok(Self { events })
    }
}
