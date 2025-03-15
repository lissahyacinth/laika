use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

pub struct TestCase {
    pub name: String,
    pub config: PathBuf,
    pub input: PathBuf,
    pub expected_output: PathBuf,
    temp_dir: TempDir,
}

impl TestCase {
    pub fn new(name: &str, config: &str, input: &str, expected: &str) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        // Set up paths relative to fixtures directory
        Self {
            name: name.to_string(),
            config: test_dir.join("configs").join(config),
            input: test_dir.join("inputs").join(input),
            expected_output: test_dir.join("outputs").join(expected),
            temp_dir,
        }
    }

    pub fn output_path(&self) -> PathBuf {
        self.temp_dir.path().join("output.jsonl")
    }

    pub fn config(&self) -> String {
        tracing::debug!("{:?}", self.config.as_path());
        fs::read_to_string(self.config.as_path()).unwrap()
    }

    pub fn compare_output(&self) -> Result<(), Box<dyn Error>> {
        let actual = fs::read_to_string(self.output_path())?;
        let expected = fs::read_to_string(&self.expected_output)?;
        tracing::debug!("Comparing {:?} to {:?}", actual, expected);
        // Compare JSONL files line by line, normalizing if needed
        compare_jsonl(&actual, &expected)
    }
}

use serde_json::{from_str, Value};
use std::error::Error;
use std::fmt;

#[derive(Debug)]
struct JsonCompareError {
    line: usize,
    details: String,
}

impl fmt::Display for JsonCompareError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "JSON comparison failed at line {}: {}",
            self.line, self.details
        )
    }
}

impl Error for JsonCompareError {}

fn normalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            // Create a new sorted map
            let mut sorted: Vec<_> = map.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));

            // Recursively normalize values
            let normalized: serde_json::Map<String, Value> = sorted
                .into_iter()
                .map(|(k, v)| (k.clone(), normalize_json(v)))
                .collect();

            Value::Object(normalized)
        }
        Value::Array(arr) => {
            // Recursively normalize array elements
            Value::Array(arr.iter().map(normalize_json).collect())
        }
        // Other JSON types (strings, numbers, booleans, null) remain unchanged
        _ => value.clone(),
    }
}

pub fn compare_jsonl(actual: &str, expected: &str) -> Result<(), Box<dyn Error>> {
    let actual_lines: Vec<_> = actual.lines().enumerate().collect();
    let expected_lines: Vec<_> = expected.lines().collect();

    if actual_lines.len() != expected_lines.len() {
        return Err(Box::new(JsonCompareError {
            line: 0,
            details: format!(
                "Line count mismatch: actual {} vs expected {}",
                actual_lines.len(),
                expected_lines.len()
            ),
        }));
    }

    for (line_num, (actual_line, expected_line)) in actual_lines
        .into_iter()
        .zip(expected_lines.iter())
        .enumerate()
    {
        // Parse both lines as JSON
        let actual_json: Value = from_str(actual_line.1).map_err(|e| JsonCompareError {
            line: line_num + 1,
            details: format!("Failed to parse actual JSON: {}", e),
        })?;

        let expected_json: Value = from_str(expected_line).map_err(|e| JsonCompareError {
            line: line_num + 1,
            details: format!("Failed to parse expected JSON: {}", e),
        })?;

        // Normalize both JSON values
        let normalized_actual = normalize_json(&actual_json);
        let normalized_expected = normalize_json(&expected_json);

        // Compare normalized values
        if normalized_actual != normalized_expected {
            return Err(Box::new(JsonCompareError {
                line: line_num + 1,
                details: format!(
                    "JSON objects differ.\nActual: {}\nExpected: {}",
                    serde_json::to_string_pretty(&normalized_actual)?,
                    serde_json::to_string_pretty(&normalized_expected)?
                ),
            }));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matching_jsonl() {
        let actual = r#"{"b": 1, "a": 2}
{"x": [2, 1], "y": 3}"#;
        let expected = r#"{"a": 2, "b": 1}
{"x": [2, 1], "y": 3}"#;

        assert!(compare_jsonl(actual, expected).is_ok());
    }

    #[test]
    fn test_mismatched_jsonl() {
        let actual = r#"{"b": 1, "a": 2}
{"x": [1, 2], "y": 3}"#;
        let expected = r#"{"a": 2, "b": 1}
{"x": [2, 1], "y": 3}"#;

        assert!(compare_jsonl(actual, expected).is_err());
    }
}
