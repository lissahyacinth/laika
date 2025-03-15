use crate::config::{
    EventCorrelation, EventProcessorConfig, EventProcessorConfigBuilder, EventTrigger,
};
use crate::connections::ConnectionConfig;
use crate::errors::{LaikaError, LaikaResult};
use crate::matcher::builder::{EventMatchBuilder, MatchPatternBuilder};
use crate::matcher::EventType;
use crate::template::error::TemplateError;
use crate::template::Template;
use crate::utils::parse_time::parse_time_str;
use serde::Deserialize;
use std::collections::HashMap;
use time::{Duration, OffsetDateTime};

#[derive(Clone, Deserialize)]
pub struct EventProcessorYamlSpec {
    pub correlation: CorrelationConfig,
    pub connections: HashMap<String, ConnectionConfig>,
    pub events: EventMatchBuilder,
    pub triggers: HashMap<String, TriggerConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchOptionsBuilder {
    /// Connector to source this Event from
    pub from: String,

    #[serde(default)]
    pub match_all: Option<serde_yaml::Value>,

    // For matchKey: { type: "..." }
    #[serde(default)]
    pub match_key: Option<HashMap<String, MatchPatternBuilder>>,
}

impl TryFrom<&EventProcessorYamlSpec> for EventProcessorConfig {
    type Error = LaikaError;

    fn try_from(value: &EventProcessorYamlSpec) -> LaikaResult<Self> {
        let event_correlation = EventCorrelation::new(
            value
                .correlation
                .events
                .clone()
                .into_iter()
                .map(|(event_type, correlation_builder)| (event_type, correlation_builder.key))
                .collect::<HashMap<EventType, String>>(),
        );
        let event_matcher = value
                .events
                .clone()
                .build()?;
        let event_triggers: HashMap<EventType, EventTrigger> = value
            .triggers
            .clone()
            .into_iter()
            .map(|(event_type, trigger_config)| {
                trigger_config
                    .try_into()
                    .map(|trigger_config| (event_type, trigger_config))
            })
            .collect::<LaikaResult<HashMap<EventType, EventTrigger>>>()?;
        Ok(EventProcessorConfigBuilder::new()
            .with_correlation(event_correlation)
            .with_event_matcher(event_matcher)
            .with_triggers(event_triggers)
            .build())
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ContentConfig {
    output: String,
}

#[derive(Deserialize, Clone)]
pub struct TimingConfigBuilder {
    from: Option<String>,
    check_every: Option<String>,
    until: Option<String>,
}

impl TimingConfigBuilder {
    pub(crate) fn parse(&self) -> LaikaResult<TimingConfig> {
        Ok(TimingConfig {
            from: self
                .from
                .as_ref()
                .map(|s| parse_time_str(s.as_str()))
                .transpose()?
                .unwrap_or(Duration::seconds(0)),
            check_every: self
                .check_every
                .as_ref()
                .map(|s| parse_time_str(s.as_str()))
                .transpose()?,
            until: self
                .until
                .as_ref()
                .map(|s| parse_time_str(s.as_str()))
                .transpose()?,
        })
    }
}

#[derive(Clone)]
pub struct TimingConfig {
    from: Duration,
    check_every: Option<Duration>,
    until: Option<Duration>,
}

impl TimingConfig {
    pub fn next_check(&self, when_requirements_were_met: OffsetDateTime) -> Option<OffsetDateTime> {
        let now = OffsetDateTime::now_utc();
        let start_time = when_requirements_were_met + self.from;
        let end_time = self.until.map(|d| when_requirements_were_met + d);

        // Check if we're past the end time
        if let Some(end) = end_time {
            if now >= end {
                return None;
            }
        }

        // If we haven't reached start time yet
        if now < start_time {
            return Some(start_time);
        }

        // Calculate next interval if check_every is specified
        self.check_every.and_then(|interval| {
            let elapsed = now - start_time;
            let intervals_passed =
                (elapsed.whole_seconds() as f64 / interval.whole_seconds() as f64).ceil() as i64;

            let seconds_to_add = interval.whole_seconds() * intervals_passed;
            let next_time = start_time + time::Duration::seconds(seconds_to_add);

            // Ensure next_time is before end_time
            if let Some(end) = end_time {
                if next_time >= end {
                    return None;
                }
            }
            Some(next_time)
        })
    }
}

#[derive(Clone, Deserialize)]
pub struct CorrelationConfig {
    #[serde(flatten)]
    pub(crate) events: HashMap<String, EventCorrelationBuilder>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum RequirementConfig {
    Exact { exact: Vec<String> },
    AtLeast { at_least: Vec<String> },
}

#[derive(Debug, Deserialize, Clone)]
pub struct RoutingConfig {
    topic: String,
}

#[derive(Deserialize, Clone)]
pub struct TriggerConfig {
    pub(crate) requires: RequirementConfig,
    #[serde(rename = "filterAndExtract")]
    pub(crate) filter_and_extract: Option<String>,
    pub(crate) timing: Option<TimingConfigBuilder>,
    pub(crate) action: ActionConfigYaml,
}

impl TryFrom<TriggerConfig> for EventTrigger {
    type Error = LaikaError;
    fn try_from(value: TriggerConfig) -> LaikaResult<Self> {
        Ok(EventTrigger {
            requirement: value.requires.into(),
            filter_and_extract: value.filter_and_extract,
            timing: value.timing.map(|v| v.parse()).transpose()?,
            action: value.action.try_into().map_err(LaikaError::from)?,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct EmitConfig {
    target: String,
    routing: RoutingConfig,
    #[serde(default)]
    payload: serde_yaml::Value,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ActionConfigYaml {
    target: String,
    payload: serde_yaml::Value,
}

#[derive(Debug, Clone)]
pub struct ActionConfig {
    pub(crate) target: String,
    pub emit_template: Template,
}

impl TryFrom<ActionConfigYaml> for ActionConfig {
    type Error = TemplateError;
    fn try_from(value: ActionConfigYaml) -> Result<Self, Self::Error> {
        Ok(ActionConfig {
            target: value.target,
            emit_template: Template::from_payload(&value.payload)?,
        })
    }
}

#[derive(Clone, Deserialize)]
pub(crate) struct EventCorrelationBuilder {
    pub(crate) key: String, // JSONPath expression
}
