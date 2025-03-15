use laika_combiner::action::{EmitAction, EventAction};
use laika_combiner::event::RawEvent;
use laika_combiner::event_handler::handle_raw_event;
use laika_combiner::event_processor::processor::EventProcessor;
use laika_combiner::storage::StorageKVBuilder;
use serde_json::{json, Value};
use std::env::temp_dir;
use std::fs;
use std::path::PathBuf;

pub mod test_utils;

pub fn process_file(processor: EventProcessor, jsonl_file: PathBuf) -> Vec<Value> {
    let file_str = fs::read_to_string(&jsonl_file).unwrap();
    let lines: Vec<_> = file_str.lines().collect();
    let mut processors = vec![processor];
    let tmp_path = temp_dir();
    let mut storage_kv = StorageKVBuilder::new(tmp_path).build().unwrap();
    storage_kv.delete_all_keys().unwrap();
    let mut outputs: Vec<Value> = vec![];
    for line in lines {
        let raw_event = RawEvent::new(serde_json::from_str::<Value>(&line).unwrap());
        match handle_raw_event(processors.as_mut_slice(), &mut storage_kv, raw_event) {
            Ok(actions) => {
                for action in actions {
                    if let EventAction::Emit(e) = action {
                        outputs.push(e.payload());
                    }
                }
            }
            Err(e) => {
                tracing::error!("{:?}", e);
            }
        }
    }
    outputs
}
