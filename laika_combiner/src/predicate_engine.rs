use crate::errors::{LaikaError, LaikaResult};
use crate::event::context::EventContext;
use crate::event::Trigger;
use deno_core::_ops::RustToV8;
use deno_core::{
    error::{CoreError, JsError},
    serde_v8, JsRuntime, RuntimeOptions,
};
use serde_json::Value as JsonValue;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JsonPredicateError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JavaScript error: {0}")]
    Js(#[from] JsError),
    #[error("Expected boolean result, got {0}")]
    NonBooleanResult(String),
    #[error("Execution error: {0}")]
    Execution(String),
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

#[derive(Debug, Clone)]
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

    pub fn store_predicate(&mut self, js_code: &str) -> JsonPredicate {
        self.predicate_count += 1;
        let id = format!("pred_{}", self.predicate_count);

        let setup_code = format!(r#"globalThis['{id}'] = {js_code};"#);
        tracing::info!("Storing predicate {}", setup_code);
        let _ = self.runtime.execute_script("[store]", setup_code);
        JsonPredicate { id }
    }

    pub fn evaluate(
        &mut self,
        predicate: &JsonPredicate,
        trigger: &Trigger,
        context: &EventContext,
    ) -> LaikaResult<Option<JsonValue>> {
        let trigger_json = serde_json::to_string(trigger).unwrap();
        let context_json = serde_json::to_string(context).unwrap();

        tracing::info!("Saving {}", trigger_json);

        let eval_code = format!(
            r#"globalThis['{id}']({trigger_json}, {context_json})"#,
            id = predicate.id,
            trigger_json = trigger_json,
            context_json = context_json
        );

        tracing::debug!("Evaluating {}", eval_code);

        let result = self.runtime.execute_script("[evaluate]", eval_code)?;
        let scope = &mut self.runtime.handle_scope();
        let local_result = result.to_v8(scope);
        if local_result.is_null() {
            Ok(None)
        } else {
            Ok(Some(
                serde_v8::from_v8::<JsonValue>(scope, local_result).map_err(|e| {
                    LaikaError::Generic(format!(
                        "Failed to convert v8::Value to serde_json::Value: {}",
                        e
                    ))
                })?,
            ))
        }
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
    use crate::event::{Event, RawEvent};
    use tracing_test::traced_test;

    #[test]
    #[traced_test]
    fn test_basic_predicate() -> LaikaResult<()> {
        let mut engine = JsonPredicateEngine::new();
        let predicate = engine
            .store_predicate("(trigger, ctx) => trigger.event.active === true ? trigger : null");
        let events: Vec<Event> = vec![
            RawEvent::new(serde_json::json!({"active": true})).parse("ActiveEvent", None),
            RawEvent::new(serde_json::json!({"active": false})).parse("InactiveEvent", None),
        ];
        let ctx = EventContext::try_from(events.clone()).unwrap();

        let trigger = Trigger::ReceivedEvent(events[0].clone());
        let evaluation_result = engine.evaluate(&predicate, &trigger, &ctx)?;
        assert!(evaluation_result.is_some());

        let trigger = Trigger::ReceivedEvent(events[1].clone());
        let evaluation_result = engine.evaluate(&predicate, &trigger, &ctx)?;
        dbg!(evaluation_result);
        assert!(engine.evaluate(&predicate, &trigger, &ctx)?.is_none());
        Ok(())
    }

    #[test]
    #[traced_test]
    fn test_string_predicate() -> LaikaResult<()> {
        let mut engine = JsonPredicateEngine::new();
        let predicate = engine
            .store_predicate("(trigger, ctx) => trigger.event.type === 'test' ? trigger: null");
        let events: Vec<Event> = vec![
            RawEvent::new(serde_json::json!({"type": "test"})).parse("ActiveEvent", None),
            RawEvent::new(serde_json::json!({"type": "not-test"})).parse("InactiveEvent", None),
        ];
        let ctx = EventContext::try_from(events.clone()).unwrap();

        let trigger = Trigger::ReceivedEvent(events[0].clone());
        let evaluation_result = engine.evaluate(&predicate, &trigger, &ctx)?;
        assert!(evaluation_result.is_some());

        let trigger = Trigger::ReceivedEvent(events[1].clone());
        let evaluation_result = engine.evaluate(&predicate, &trigger, &ctx)?;
        assert!(evaluation_result.is_none());
        Ok(())
    }
}
