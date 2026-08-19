//! Runtime-neutral orchestration state for an agent loop.
//!
//! Callers own model and tool I/O. This state machine owns budget stops,
//! model retry accounting and the normal/abnormal lifecycle boundary.

use crate::agent_loop::{
    AgentDirective, AgentLoopCheckpoint, AgentLoopEngine, AgentLoopLimits, AgentLoopStop,
    parse_agent_directive,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelAttemptOutcome {
    Accepted,
    Retry { attempt: usize, backoff_ms: u64 },
    Exhausted { attempts: usize },
}

#[derive(Debug)]
pub struct AgentOrchestrator {
    loop_engine: AgentLoopEngine,
    max_consecutive_model_errors: usize,
    consecutive_model_errors: usize,
    stop_reason: Option<AgentLoopStop>,
    finished_normally: bool,
}

impl AgentOrchestrator {
    pub fn new(limits: AgentLoopLimits, max_consecutive_model_errors: usize) -> Self {
        Self::from_checkpoint(
            limits,
            AgentLoopCheckpoint::default(),
            max_consecutive_model_errors,
        )
    }

    pub fn from_checkpoint(
        limits: AgentLoopLimits,
        checkpoint: AgentLoopCheckpoint,
        max_consecutive_model_errors: usize,
    ) -> Self {
        Self {
            loop_engine: AgentLoopEngine::from_checkpoint(limits, checkpoint),
            max_consecutive_model_errors: max_consecutive_model_errors.max(1),
            consecutive_model_errors: 0,
            stop_reason: None,
            finished_normally: false,
        }
    }

    pub const fn limits(&self) -> &AgentLoopLimits {
        self.loop_engine.limits()
    }

    pub const fn checkpoint(&self) -> AgentLoopCheckpoint {
        self.loop_engine.checkpoint()
    }

    pub const fn next_round(&self) -> usize {
        self.loop_engine.next_round()
    }

    pub const fn rounds_used(&self) -> usize {
        self.loop_engine.rounds_used()
    }

    pub const fn max_consecutive_model_errors(&self) -> usize {
        self.max_consecutive_model_errors
    }

    pub const fn consecutive_model_errors(&self) -> usize {
        self.consecutive_model_errors
    }

    pub const fn stop_reason(&self) -> Option<AgentLoopStop> {
        self.stop_reason
    }

    pub const fn finished_normally(&self) -> bool {
        self.finished_normally
    }

    pub fn begin_round(&mut self, round: usize) -> Result<(), AgentLoopStop> {
        self.loop_engine.begin_round(round).inspect_err(|reason| {
            self.stop_reason = Some(*reason);
        })
    }

    pub fn start_request(&mut self) -> Result<(), AgentLoopStop> {
        self.loop_engine.start_request().inspect_err(|reason| {
            self.stop_reason = Some(*reason);
        })
    }

    pub fn record_model_error(&mut self, transient: bool) -> ModelAttemptOutcome {
        self.consecutive_model_errors = self.consecutive_model_errors.saturating_add(1);
        if transient && self.consecutive_model_errors < self.max_consecutive_model_errors {
            let backoff_ms = 500_u64
                .saturating_mul(1_u64 << self.consecutive_model_errors.saturating_sub(1).min(16))
                .min(30_000);
            ModelAttemptOutcome::Retry {
                attempt: self.consecutive_model_errors,
                backoff_ms,
            }
        } else {
            ModelAttemptOutcome::Exhausted {
                attempts: self.consecutive_model_errors,
            }
        }
    }

    pub fn record_model_output(
        &mut self,
        input_tokens: u64,
        output_tokens: u64,
        empty: bool,
    ) -> ModelAttemptOutcome {
        self.loop_engine
            .record_model_usage(input_tokens, output_tokens);
        if empty {
            self.consecutive_model_errors = self.consecutive_model_errors.saturating_add(1);
            if self.consecutive_model_errors < self.max_consecutive_model_errors {
                return ModelAttemptOutcome::Retry {
                    attempt: self.consecutive_model_errors,
                    backoff_ms: 250_u64.saturating_mul(self.consecutive_model_errors as u64),
                };
            }
            return ModelAttemptOutcome::Exhausted {
                attempts: self.consecutive_model_errors,
            };
        }
        self.consecutive_model_errors = 0;
        ModelAttemptOutcome::Accepted
    }

    pub fn record_tool_result(&mut self, success: bool, effective: bool) {
        self.loop_engine.record_tool_result(success, effective);
    }

    pub fn record_policy_block(&mut self) {
        self.loop_engine.record_policy_block();
    }

    pub fn directive(&self, content: &str) -> AgentDirective {
        parse_agent_directive(content)
    }

    pub fn finish(&mut self) {
        self.finished_normally = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> AgentLoopLimits {
        AgentLoopLimits {
            max_rounds: 3,
            max_duration_seconds: 60,
            max_requests: 2,
            max_tokens: 100,
            stall_limit: None,
        }
    }

    #[test]
    fn transient_model_errors_back_off_until_exhausted() {
        let mut orchestrator = AgentOrchestrator::new(limits(), 3);
        assert_eq!(
            orchestrator.record_model_error(true),
            ModelAttemptOutcome::Retry {
                attempt: 1,
                backoff_ms: 500
            }
        );
        assert_eq!(
            orchestrator.record_model_error(true),
            ModelAttemptOutcome::Retry {
                attempt: 2,
                backoff_ms: 1_000
            }
        );
        assert_eq!(
            orchestrator.record_model_error(true),
            ModelAttemptOutcome::Exhausted { attempts: 3 }
        );
    }

    #[test]
    fn non_transient_model_error_exhausts_immediately() {
        let mut orchestrator = AgentOrchestrator::new(limits(), 3);
        assert_eq!(
            orchestrator.record_model_error(false),
            ModelAttemptOutcome::Exhausted { attempts: 1 }
        );
    }

    #[test]
    fn accepted_output_resets_empty_response_counter() {
        let mut orchestrator = AgentOrchestrator::new(limits(), 3);
        assert_eq!(
            orchestrator.record_model_output(10, 0, true),
            ModelAttemptOutcome::Retry {
                attempt: 1,
                backoff_ms: 250
            }
        );
        assert_eq!(
            orchestrator.record_model_output(10, 5, false),
            ModelAttemptOutcome::Accepted
        );
        assert_eq!(orchestrator.consecutive_model_errors(), 0);
        assert_eq!(orchestrator.checkpoint().token_count, 25);
    }

    #[test]
    fn budget_stop_and_finish_are_owned_by_orchestrator() {
        let mut orchestrator = AgentOrchestrator::new(limits(), 3);
        orchestrator.start_request().unwrap();
        orchestrator.start_request().unwrap();
        assert_eq!(orchestrator.start_request(), Err(AgentLoopStop::Requests));
        assert_eq!(orchestrator.stop_reason(), Some(AgentLoopStop::Requests));
        assert!(!orchestrator.finished_normally());
        orchestrator.finish();
        assert!(orchestrator.finished_normally());
    }
}
