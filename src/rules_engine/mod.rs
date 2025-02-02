use std::{fs, path::Path};
use deno_core::{error::{CoreError, JsError}, JsRuntime, RuntimeOptions};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JsonPredicateError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JavaScript error: {0}")]
    Js(#[from] JsError),
    #[error("Execution failed: {0}")]
    Execution(String),
    #[error("Expected boolean result, got {0}")]
    NonBooleanResult(String),
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

/// A stored JavaScript predicate function
pub struct JsonPredicate {
    id: String,
}

/// Engine for running JSON predicates
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

    /// Store a predicate without validation
    pub fn store_predicate(&mut self, js_code: &str) -> JsonPredicate {
        self.predicate_count += 1;
        let id = format!("pred_{}", self.predicate_count);

        // Just store the function directly
        let setup_code = format!(
            r#"globalThis['{id}'] = {js_code};"#
        );

        let _ = self.runtime.execute_script("[store]", setup_code);
        JsonPredicate { id }
    }

    /// Evaluate a predicate against JSON data
    pub fn evaluate(
        &mut self,
        predicate: &JsonPredicate,
        json_str: &str,
    ) -> Result<bool, JsonPredicateError> {
        let eval_code = format!(
            r#"globalThis['{id}'](JSON.parse('{json}'))"#,
            id = predicate.id,
            json = json_str.replace('\'', "\\'")
        );

        let result = self.runtime.execute_script("[evaluate]", eval_code)?;
        let scope = &mut self.runtime.handle_scope();
        let value = result.open(scope);

        if !value.is_boolean() {
            let type_name = if value.is_string() {
                "string"
            } else if value.is_number() {
                "number"
            } else if value.is_object() {
                "object"
            } else if value.is_undefined() {
                "undefined"
            } else if value.is_null() {
                "null"
            } else {
                "unknown"
            };

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

        let predicate = engine.store_predicate(
            r#"(data) => data.active === true"#,
        );

        assert!(engine.evaluate(&predicate, r#"{"active": true}"#)?);
        assert!(!engine.evaluate(&predicate, r#"{"active": false}"#)?);
        Ok(())
    }

    #[test]
    fn test_membership_predicate() -> Result<(), JsonPredicateError> {
        let mut engine = JsonPredicateEngine::new();

        let predicate = engine.store_predicate(
            r#"
            (data) => {
                if (!data.user?.type) return false;
                return ['premium', 'enterprise'].includes(data.user.type);
            }
            "#,
        );

        assert!(engine.evaluate(&predicate, r#"{"user": {"type": "premium"}}"#)?);
        assert!(!engine.evaluate(&predicate, r#"{"user": {"type": "basic"}}"#)?);
        Ok(())
    }

    #[test]
    fn test_date_predicate() -> Result<(), JsonPredicateError> {
        let mut engine = JsonPredicateEngine::new();

        let predicate = engine.store_predicate(
            r#"
            (data) => {
                const { user } = data;
                if (!user?.memberSince) return false;

                const membershipAge = new Date() - new Date(user.memberSince);
                const thirtyDays = 30 * 24 * 60 * 60 * 1000;

                return membershipAge < thirtyDays;
            }
            "#,
        );

        assert!(engine.evaluate(
            &predicate,
            r#"{
            "user": {
                "memberSince": "2024-01-30T00:00:00Z"
            }
        }"#
        )?);

        assert!(!engine.evaluate(
            &predicate,
            r#"{
            "user": {
                "memberSince": "2023-01-01T00:00:00Z"
            }
        }"#
        )?);

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
            let result = engine.evaluate(&predicate, "{}");

            assert!(matches!(
                result,
                Err(JsonPredicateError::NonBooleanResult(ref t)) if t == expected_type
            ));
        }

        Ok(())
    }
}