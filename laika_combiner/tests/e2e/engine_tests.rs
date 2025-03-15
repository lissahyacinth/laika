use crate::common::process_file;
use crate::common::test_utils::TestCase;
use laika_combiner::config::builder::EventProcessorYamlSpec;
use laika_combiner::config::EventProcessorConfig;
use laika_combiner::EventProcessor;
use std::fs::File;
use std::io::Write;

#[test]
pub fn test_single_event_processing() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let test_case = TestCase::new(
        "single_event",
        "basic.yaml",
        "single_event.jsonl",
        "single_event_output.jsonl",
    );

    let processor_spec: EventProcessorYamlSpec = serde_yaml::from_str(&test_case.config()).unwrap();
    let processor: EventProcessor = EventProcessorConfig::try_from(&processor_spec)
        .unwrap()
        .build();
    let result = process_file(processor, test_case.input.clone());
    let mut result_file = File::create(test_case.output_path()).unwrap();
    for value in result {
        let line = serde_json::to_string(&value).unwrap();
        writeln!(result_file, "{}", line).unwrap();
    }
    if let Err(e) = test_case.compare_output() {
        tracing::error!("{}", e);
        assert!(false);
    }
}
