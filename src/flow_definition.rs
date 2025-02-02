use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize)]
pub struct EventRuleBuilder {
    pub correlation: Correlation,
    pub events: HashMap<String, EventDefinition>,
    pub flow: Flow,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Correlation {
    pub key: HashMap<String, String>, // eventName -> jsonPath
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventDefinition {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Flow {
    pub conditions: HashMap<String, Condition>,
    #[serde(flatten)]
    pub cases: HashMap<String, Case>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Condition {
    #[serde(rename = "timingCondition")]
    Timing {
        event: String,
        within: String, // We'll parse this into Duration after
        #[serde(rename = "startFrom")]
        start_from: StartFrom,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartFrom {
    FirstEvent,
    LastEvent,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Case {
    pub requires: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<ConditionExpr>,
    pub action: Action,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConditionExpr {
    Reference(String),
    Not {
        #[serde(rename = "not")]
        expr: Box<ConditionExpr>,
    },
    And {
        #[serde(rename = "and")]
        exprs: Vec<ConditionExpr>,
    },
    Or {
        #[serde(rename = "or")]
        exprs: Vec<ConditionExpr>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Action {
    #[serde(rename = "type")]
    pub action_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Error, Debug)]
pub enum RuleError {
    #[error("YAML parsing error: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("Invalid duration format: {0}")]
    DurationError(String),
    #[error("Referenced condition not found: {0}")]
    MissingCondition(String),
    #[error("Referenced event not found: {0}")]
    MissingEvent(String),
}

impl EventRuleBuilder {
    pub fn from_yaml(yaml: &str) -> Result<Self, RuleError> {
        let rule: EventRuleBuilder = serde_yaml::from_str(yaml)?;
        rule.validate()?;
        Ok(rule)
    }

    pub fn validate(&self) -> Result<(), RuleError> {
        // Validate all referenced events exist
        for case in self.flow.cases.values() {
            for event in &case.requires {
                if !self.events.contains_key(event) {
                    return Err(RuleError::MissingEvent(event.clone()));
                }
            }
        }

        // Validate all referenced conditions exist
        for case in self.flow.cases.values() {
            if let Some(condition) = &case.condition {
                self.validate_condition_expr(condition)?;
            }
        }

        // Validate correlation keys reference valid events
        for event in self.correlation.key.keys() {
            if !self.events.contains_key(event) {
                return Err(RuleError::MissingEvent(event.clone()));
            }
        }

        Ok(())
    }

    fn validate_condition_expr(&self, expr: &ConditionExpr) -> Result<(), RuleError> {
        match expr {
            ConditionExpr::Reference(name) => {
                if !self.flow.conditions.contains_key(name) {
                    return Err(RuleError::MissingCondition(name.clone()));
                }
            }
            ConditionExpr::Not { expr } => {
                self.validate_condition_expr(expr)?;
            }
            ConditionExpr::And { exprs } | ConditionExpr::Or { exprs } => {
                for expr in exprs {
                    self.validate_condition_expr(expr)?;
                }
            }
        }
        Ok(())
    }
}

pub fn parse_duration(duration_str: &str) -> Result<Duration, RuleError> {
    // Trim whitespace and standardize to lowercase
    let duration_str = duration_str.trim().to_lowercase();

    // Separate numbers and unit, handling optional whitespace
    let numeric_part: String = duration_str
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let unit_part: String = duration_str
        .chars()
        .skip(numeric_part.len())
        .filter(|c| !c.is_whitespace())
        .collect();

    let number = numeric_part.parse::<u64>().map_err(|_| {
        RuleError::DurationError(format!("Invalid duration number: {}", duration_str))
    })?;

    match unit_part.as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => Ok(Duration::from_secs(number)),
        "m" | "min" | "mins" | "minute" | "minutes" => Ok(Duration::from_secs(number * 60)),
        "h" | "hr" | "hrs" | "hour" | "hours" => Ok(Duration::from_secs(number * 3600)),
        "d" | "day" | "days" => Ok(Duration::from_secs(number * 86400)),
        _ => Err(RuleError::DurationError(format!(
            "Invalid duration unit: {}",
            duration_str
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_rule() {
        let yaml = r#"
correlation:
  key:
    eventA: "$.transaction_id"
    eventB: "$.txn_id"
    eventC: "$.transaction_id"

events:
  eventA:
    type: "PaymentInitiated"
  eventB:
    type: "PaymentAuthorized"
  eventC:
    type: "PaymentSettled"

flow:
  conditions:
    settledInTime:
      type: "timingCondition"
      event: "eventC"
      within: "30m"
      startFrom: "firstEvent"

  logCase:
    requires:
      - eventA
    action:
      type: "httpPost"

  successCase:
    requires:
      - eventA
      - eventB
      - eventC
    condition: settledInTime
    action:
      type: "httpPost"

  errorCase:
    requires:
      - eventA
      - eventB
    condition:
      not: settledInTime
    action:
      type: "createAlert"
"#;
        let rule = EventRuleBuilder::from_yaml(yaml).unwrap();
        assert_eq!(rule.events.len(), 3);
        assert_eq!(rule.flow.cases.len(), 3);
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
        assert!(parse_duration("invalid").is_err());
    }
}
