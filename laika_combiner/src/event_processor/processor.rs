use crate::action::{EmitAction, EventAction};
use crate::broker::{CorrelationId, EventExpiry};
use crate::config::builder::ActionConfig;
use crate::config::{EventCorrelation, EventRuleDefinition};
use crate::errors::{LaikaError, LaikaResult};
use crate::event::context::EventContext;
use crate::event::{Event, EventLike, RawEvent, Trigger};
use crate::matcher::EventTypeDefinitions;
use crate::predicate_engine::JsonPredicateEngine;
use crate::rules::{EventRule, RuleResult};

pub struct EventProcessor {
    pub(crate) engine: JsonPredicateEngine,
    event_matcher: EventTypeDefinitions,
    event_correlation: EventCorrelation,
    pub rules: Vec<EventRule>,
}

impl EventProcessor {
    pub fn new(
        event_matcher: EventTypeDefinitions,
        event_correlation: EventCorrelation,
        rules: Vec<EventRuleDefinition>,
    ) -> Self {
        let mut engine = JsonPredicateEngine::new();
        let rules = rules
            .into_iter()
            .map(|rule| rule.register_to_engine(&mut engine))
            .collect();
        Self {
            engine,
            event_matcher,
            event_correlation,
            rules,
        }
    }

    /// Parse a Raw Event into all Matching Events
    pub(crate) fn parse_event(
        &self,
        event_source: &str,
        raw_event: RawEvent,
    ) -> LaikaResult<Vec<Event>> {
        let mut matched_events: Vec<Event> = Vec::new();
        for event_type in self
            .event_matcher
            .match_message(event_source, raw_event.get_data())?
        {
            matched_events.push(
                raw_event.clone().parse(
                    event_type.clone(),
                    self.event_correlation
                        .correlation_id(&event_type, &raw_event)?,
                ),
            );
        }
        Ok(matched_events)
    }

    fn emit_action(
        action_config: &ActionConfig,
        output: serde_json::Value,
    ) -> Result<EventAction, LaikaError> {
        Ok(EventAction::Emit(EmitAction::new(
            action_config.target.clone(),
            serde_json::to_value(action_config.emit_template.clone().render(&output)?)
                .map_err(|e| LaikaError::TemplateError(e.to_string()))?,
        )))
    }

    /// Actions to take given matched conditions, if any
    ///
    /// Context is the surrounding events to a given event.
    /// Trigger is the item that caused this rule to be evaluated.
    pub fn relevant_actions(
        &mut self,
        correlation_id: &Option<CorrelationId>,
        // Either a timing trigger, or a correlated event
        trigger: &Trigger,
        context: &EventContext,
    ) -> LaikaResult<Vec<EventAction>> {
        let mut actions: Vec<EventAction> = Vec::new();
        for rule in self.rules.iter() {
            match rule.evaluate(&mut self.engine, trigger, context)? {
                RuleResult::ConditionSatisfied {
                    met_at,
                    action_config,
                    condition_result,
                } => actions.push(Self::emit_action(&action_config, condition_result)?),
                RuleResult::ConditionNotSatisfied { met_at, recheck } => {
                    // Early return if any condition isn't met
                    let Some(recheck_config) = recheck else {
                        continue;
                    };
                    let Some(correlation_id) = correlation_id.clone() else {
                        continue;
                    };

                    if let Some(next_wakeup) = recheck_config.next_check(met_at) {
                        actions.push(EventAction::ScheduleWakeup(EventExpiry::new(
                            next_wakeup,
                            correlation_id,
                            rule.name.clone(),
                        )))
                    }
                }
                RuleResult::RequirementNotMet { .. } => {}
            }
        }
        Ok(actions)
    }
}
