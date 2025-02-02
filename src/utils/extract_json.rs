use crate::errors::{LaikaError, LaikaResult};
use serde_json::Value;

pub fn extract_json_field<'a>(value: &'a Value, field_path: &str) -> LaikaResult<&'a Value> {
    let path = field_path.strip_prefix('$').unwrap_or(field_path);
    let mut current = value;

    for part in path.split('.').filter(|p| !p.is_empty()) {
        current = current
            .get(part)
            .ok_or_else(|| LaikaError::FieldNotFound(part.to_string(), field_path.to_string()))?;
    }

    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_json_field() {
        let json = json!({
            "name": "test",
            "user": {
                "id": 123,
                "details": {
                    "email": "test@example.com"
                }
            }
        });

        assert_eq!(extract_json_field(&json, "$.user.id"), Ok(&json! {123}));

        assert_eq!(
            extract_json_field(&json, "user.details.email"),
            Ok(&json! {"test@example.com"})
        );

        assert!(matches!(
            extract_json_field(&json, "$.nonexistent"),
            Err(LaikaError::FieldNotFound(..))
        ));

        assert!(matches!(
            extract_json_field(&json, "$.user.details.nonexistent"),
            Err(LaikaError::FieldNotFound(..))
        ));
    }
}
