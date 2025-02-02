use std::{fs, path::Path};
use deno_core::{error::{CoreError, JsError}, v8, JsRuntime, RuntimeOptions};
use thiserror::Error;
use serde_json::Value;

#[derive(Error, Debug)]
pub enum JsonPredicateError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JavaScript error: {0}")]
    Js(#[from] JsError),
    #[error("Expected boolean result, got {0}")]
    NonBooleanResult(String),
    #[error("Execution error: {0}")]
    Execution(String)
}

impl From<CoreError> for JsonPredicateError {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::Js(js_error) => JsonPredicateError::Js(js_error),
            CoreError::Execute(spec) => JsonPredicateError::Execution(spec.to_string()),
            CoreError::Io(io_error) => JsonPredicateError::Io(io_error),
            _ => JsonPredicateError::Execution(err.to_string()),
        }
    }
}

pub struct JsonPredicate {
    id: String,
}

pub struct JsonPredicateEngine {
    runtime: JsRuntime,
    predicate_count: usize,
}

impl JsonPredicateEngine {
    pub fn new() -> Self {
        let runtime = JsRuntime::new(RuntimeOptions::default());
        JsonPredicateEngine {
            runtime,
            predicate_count: 0,
        }
    }

    pub fn load_from_file<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<JsonPredicate, JsonPredicateError> {
        let js_code = fs::read_to_string(path)?;
        Ok(self.store_predicate(&js_code))
    }

    pub fn store_predicate(&mut self, js_code: &str) -> JsonPredicate {
        self.predicate_count += 1;
        let id = format!("pred_{}", self.predicate_count);

        let setup_code = format!(
            r#"globalThis['{id}'] = {js_code};"#
        );

        let _ = self.runtime.execute_script("[store]", setup_code);
        JsonPredicate { id }
    }

    pub fn evaluate(
        &mut self,
        predicate: &JsonPredicate,
        value: &Value,
    ) -> Result<bool, JsonPredicateError> {
        let json = serde_json::to_string(value).unwrap();
        let eval_code = format!(
            r#"globalThis['{id}']({json})"#,
            id = predicate.id,
        );

        let result = self.runtime.execute_script("[evaluate]", eval_code)?;
        let scope = &mut self.runtime.handle_scope();
        let value = result.open(scope);

        if !value.is_boolean() {
            let type_name = if value.is_string() { "string" }
            else if value.is_number() { "number" }
            else if value.is_object() { "object" }
            else if value.is_undefined() { "undefined" }
            else if value.is_null() { "null" }
            else { "unknown" };

            return Err(JsonPredicateError::NonBooleanResult(type_name.to_string()));
        }

        Ok(value.boolean_value(scope))
    }
}

impl Default for JsonPredicateEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_predicate() -> Result<(), JsonPredicateError> {
        let mut engine = JsonPredicateEngine::new();
        let predicate = engine.store_predicate("(data) => data.active === true");

        let value = serde_json::json!({"active": true});
        assert!(engine.evaluate(&predicate, &value)?);

        let value = serde_json::json!({"active": false});
        assert!(!engine.evaluate(&predicate, &value)?);
        Ok(())
    }

    #[test]
    fn test_membership_predicate() -> Result<(), JsonPredicateError> {
        let mut engine = JsonPredicateEngine::new();
        let predicate = engine.store_predicate(r#"
            (data) => {
                if (!data.user?.type) return false;
                return ['premium', 'enterprise'].includes(data.user.type);
            }
        "#);

        let value = serde_json::json!({"user": {"type": "premium"}});
        assert!(engine.evaluate(&predicate, &value)?);

        let value = serde_json::json!({"user": {"type": "basic"}});
        assert!(!engine.evaluate(&predicate, &value)?);
        Ok(())
    }

    #[test]
    fn test_json_with_special_chars() -> Result<(), JsonPredicateError> {
        let mut engine = JsonPredicateEngine::new();
        let predicate = engine.store_predicate(r#"(data) => data.text === "hello\nworld""#);

        let value = serde_json::json!({
            "text": "hello\nworld"
        });

        assert!(engine.evaluate(&predicate, &value)?);
        Ok(())
    }

    #[test]
    fn test_type_errors() -> Result<(), JsonPredicateError> {
        let mut engine = JsonPredicateEngine::new();

        let test_cases = [
            ("(data) => 'string'", "string"),
            ("(data) => 42", "number"),
            ("(data) => undefined", "undefined"),
            ("(data) => null", "null"),
            ("(data) => ({})", "object"),
        ];

        for (code, expected_type) in test_cases {
            let predicate = engine.store_predicate(code);
            let value = serde_json::json!({});
            let result = engine.evaluate(&predicate, &value);

            assert!(matches!(
                result,
                Err(JsonPredicateError::NonBooleanResult(ref t)) if t == expected_type
            ));
        }

        Ok(())
    }
}

