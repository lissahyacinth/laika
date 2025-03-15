use crate::connections::{EventReceiver, EventSubmitter, MessagingError, RoutingConfig};
use async_trait::async_trait;
use serde_json::Value;
use std::fs::{File};
use std::path::Path;
use tokio::fs::File as TokioFile;
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader, BufWriter as TokioBufWriter,
};
use tokio::sync::Mutex;

pub struct FileEventQueue {
    file_path: String,
    reader: Mutex<TokioBufReader<TokioFile>>,
    writer: Mutex<TokioBufWriter<TokioFile>>,
}

impl FileEventQueue {
    pub async fn new(file_path: &str) -> Result<Self, MessagingError> {
        if !Path::new(file_path).exists() {
            File::create(file_path).map_err(MessagingError::Io)?;
        }

        let reader_file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(file_path)
            .await?;

        let writer_file = tokio::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(file_path)
            .await?;

        Ok(Self {
            file_path: file_path.to_string(),
            reader: Mutex::new(TokioBufReader::new(reader_file)),
            writer: Mutex::new(TokioBufWriter::new(writer_file)),
        })
    }
}

#[async_trait]
impl EventSubmitter for FileEventQueue {
    async fn submit(&self, payload: Value) -> Result<(), MessagingError> {
        let json_string = serde_json::to_string(&payload).map_err(MessagingError::Json)?;
        let mut writer = self.writer.lock().await;
        writer.write_all(json_string.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok(())
    }
}

#[async_trait]
impl EventReceiver for FileEventQueue {
    async fn receive_one(&self) -> Result<Option<Value>, MessagingError> {
        let mut reader = self.reader.lock().await;
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            return Err(MessagingError::StreamFinished);
        }
        let value: Value = serde_json::from_str(line.trim())?;
        Ok(Some(value))
    }
}
