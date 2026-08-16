//! Configuration-driven control plane shared by main and sub-agent loops.
//!
//! Adapters still perform model, persistence and tool I/O. `AgentRunner` owns
//! round advancement, budget accounting and the common completion gates.

use crate::agent_loop::{AgentDirective, AgentLoopCheckpoint, AgentLoopLimits, AgentLoopStop};
use crate::orchestrator::{AgentOrchestrator, ModelAttemptOutcome};
use crate::ports::ModelTurnOutput;

#[derive(Debug, Clone, Copy)]
pub struct AgentRunConfig {
    pub limits: AgentLoopLimits,
    pub max_consecutive_model_errors: usize,
    pub allow_sub_agents: bool,
    pub emit_turn_events: bool,
    pub completion_policy: CompletionPolicy,
    pub observation_policy: ObservationPolicy,
}

impl AgentRunConfig {
    pub const fn main(limits: AgentLoopLimits, max_consecutive_model_errors: usize) -> Self {
        Self {
            limits,
            max_consecutive_model_errors,
            allow_sub_agents: true,
            emit_turn_events: true,
            completion_policy: CompletionPolicy::main(),
            observation_policy: ObservationPolicy {
                require_success_for_effective: true,
            },
        }
    }

    pub const fn sub_agent(limits: AgentLoopLimits) -> Self {
        Self {
            limits,
            max_consecutive_model_errors: 1,
            allow_sub_agents: false,
            emit_turn_events: false,
            completion_policy: CompletionPolicy::sub_agent(),
            observation_policy: ObservationPolicy {
                require_success_for_effective: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CompletionPolicy {
    pub minimum_tool_calls: usize,
    pub require_effective_observation: bool,
    pub accept_implicit_candidate: bool,
}

impl CompletionPolicy {
    pub const fn main() -> Self {
        Self {
            minimum_tool_calls: 0,
            require_effective_observation: false,
            accept_implicit_candidate: true,
        }
    }

    pub const fn sub_agent() -> Self {
        Self {
            minimum_tool_calls: 1,
            require_effective_observation: true,
            accept_implicit_candidate: true,
        }
    }

    pub fn rejection(
        self,
        directive: &AgentDirective,
        checkpoint: AgentLoopCheckpoint,
    ) -> Option<CompletionRejection> {
        if !matches!(
            directive,
            AgentDirective::Final(_) | AgentDirective::Candidate(_)
        ) {
            return None;
        }
        if checkpoint.tool_call_count < self.minimum_tool_calls {
            return Some(CompletionRejection::InsufficientToolCalls {
                required: self.minimum_tool_calls,
                actual: checkpoint.tool_call_count,
            });
        }
        if self.require_effective_observation && checkpoint.effective_observations == 0 {
            return Some(CompletionRejection::NoEffectiveObservation);
        }
        if !self.accept_implicit_candidate && matches!(directive, AgentDirective::Candidate(_)) {
            return Some(CompletionRejection::ExplicitFinalRequired);
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionRejection {
    InsufficientToolCalls { required: usize, actual: usize },
    NoEffectiveObservation,
    ExplicitFinalRequired,
}

#[derive(Debug, Clone, Copy)]
pub struct ObservationPolicy {
    pub require_success_for_effective: bool,
}

impl ObservationPolicy {
    pub const fn project(self, success: bool, effective: bool) -> (bool, bool) {
        (
            success,
            effective && (!self.require_success_for_effective || success),
        )
    }
}

impl Default for ObservationPolicy {
    fn default() -> Self {
        Self {
            require_success_for_effective: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRoundOutcome<T> {
    Continue,
    Complete(T),
    /// Stop abnormally and let the adapter perform budget finalization.
    Halt,
}

#[derive(Debug, Clone)]
pub struct AgentRunResult<T> {
    pub output: Option<T>,
    pub checkpoint: AgentLoopCheckpoint,
    pub stop_reason: Option<AgentLoopStop>,
    pub finished_normally: bool,
}

/// Adapter-owned model, tool and persistence behavior for one agent round.
///
/// The runtime owns loop advancement and terminal accounting. Implementations
/// only describe what happens inside a round and how an abnormal stop is
/// projected into an optional adapter result.
#[async_trait::async_trait]
pub trait AgentRunHooks: Send {
    type Output: Send;

    async fn run_round(
        &mut self,
        round: usize,
        runner: &mut AgentRunner,
    ) -> anyhow::Result<AgentRoundOutcome<Self::Output>>;

    async fn stopped(
        &mut self,
        _reason: AgentLoopStop,
        _checkpoint: AgentLoopCheckpoint,
        _runner: &AgentRunner,
    ) -> anyhow::Result<Option<Self::Output>> {
        Ok(None)
    }
}

/// Adapter hooks for the I/O portions of a model-driven agent round.
///
/// The runtime owns the common ordering and accounting: prepare, invoke the
/// model, apply retry policy, then route the accepted output to either tool
/// execution or lifecycle-directive handling. Server adapters only implement
/// the side effects at those boundaries.
#[async_trait::async_trait]
pub trait AgentRoundHooks: Send {
    type Output: Send;

    async fn prepare_round(&mut self, _round: usize) -> anyhow::Result<()> {
        Ok(())
    }

    async fn generate(&mut self, round: usize) -> anyhow::Result<ModelTurnOutput>;

    fn is_transient_model_error(&self, _error: &anyhow::Error) -> bool {
        false
    }

    async fn model_retry(
        &mut self,
        _round: usize,
        _error: &anyhow::Error,
        _attempt: usize,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn model_exhausted(
        &mut self,
        _round: usize,
        _error: &anyhow::Error,
        _transient: bool,
        _attempts: usize,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn empty_model_retry(&mut self, _round: usize, _attempt: usize) -> anyhow::Result<()> {
        Ok(())
    }

    async fn empty_model_exhausted(
        &mut self,
        _round: usize,
        _attempts: usize,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Handle an accepted model output. `directive` is `None` for tool-call
    /// output and populated for a tool-free lifecycle response.
    async fn handle_output(
        &mut self,
        round: usize,
        output: ModelTurnOutput,
        directive: Option<AgentDirective>,
        runner: &mut AgentRunner,
    ) -> anyhow::Result<AgentRoundOutcome<Self::Output>>;
}

#[derive(Debug)]
pub struct AgentRunner {
    config: AgentRunConfig,
    orchestrator: AgentOrchestrator,
}

impl AgentRunner {
    pub fn new(config: AgentRunConfig) -> Self {
        Self::from_checkpoint(config, AgentLoopCheckpoint::default())
    }

    pub fn from_checkpoint(config: AgentRunConfig, checkpoint: AgentLoopCheckpoint) -> Self {
        Self {
            orchestrator: AgentOrchestrator::from_checkpoint(
                config.limits,
                checkpoint,
                config.max_consecutive_model_errors,
            ),
            config,
        }
    }

    pub const fn config(&self) -> &AgentRunConfig {
        &self.config
    }

    pub const fn limits(&self) -> &AgentLoopLimits {
        self.orchestrator.limits()
    }

    pub const fn checkpoint(&self) -> AgentLoopCheckpoint {
        self.orchestrator.checkpoint()
    }

    pub const fn stop_reason(&self) -> Option<AgentLoopStop> {
        self.orchestrator.stop_reason()
    }

    pub const fn rounds_used(&self) -> usize {
        self.orchestrator.rounds_used()
    }

    pub const fn finished_normally(&self) -> bool {
        self.orchestrator.finished_normally()
    }

    pub const fn max_consecutive_model_errors(&self) -> usize {
        self.orchestrator.max_consecutive_model_errors()
    }

    pub fn begin_next_round(&mut self) -> Result<usize, AgentLoopStop> {
        let round = self.orchestrator.next_round();
        self.orchestrator.begin_round(round)?;
        Ok(round)
    }

    pub async fn run<H>(&mut self, hooks: &mut H) -> anyhow::Result<AgentRunResult<H::Output>>
    where
        H: AgentRunHooks,
    {
        loop {
            let round = match self.begin_next_round() {
                Ok(round) => round,
                Err(reason) => {
                    let output = hooks.stopped(reason, self.checkpoint(), self).await?;
                    return Ok(self.run_result(output));
                }
            };
            match hooks.run_round(round, self).await? {
                AgentRoundOutcome::Continue => {}
                AgentRoundOutcome::Complete(output) => {
                    self.finish();
                    return Ok(self.run_result(Some(output)));
                }
                AgentRoundOutcome::Halt => return Ok(self.run_result(None)),
            }
        }
    }

    /// Execute the shared model-driven stages of one main or sub-agent round.
    pub async fn run_model_round<H>(
        &mut self,
        round: usize,
        hooks: &mut H,
    ) -> anyhow::Result<AgentRoundOutcome<H::Output>>
    where
        H: AgentRoundHooks,
    {
        hooks.prepare_round(round).await?;
        let output = match hooks.generate(round).await {
            Ok(output) => output,
            Err(error) => {
                let transient = hooks.is_transient_model_error(&error);
                return match self.record_model_error(transient) {
                    ModelAttemptOutcome::Retry {
                        attempt,
                        backoff_ms,
                    } => {
                        hooks.model_retry(round, &error, attempt).await?;
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                        Ok(AgentRoundOutcome::Continue)
                    }
                    ModelAttemptOutcome::Exhausted { attempts } => {
                        hooks
                            .model_exhausted(round, &error, transient, attempts)
                            .await?;
                        Err(error)
                    }
                    ModelAttemptOutcome::Accepted => unreachable!(),
                };
            }
        };

        let empty = output.tool_calls.is_empty() && output.content.trim().is_empty();
        match self.record_model_output(output.usage.input_tokens, output.usage.output_tokens, empty)
        {
            ModelAttemptOutcome::Retry {
                attempt,
                backoff_ms,
            } => {
                hooks.empty_model_retry(round, attempt).await?;
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                Ok(AgentRoundOutcome::Continue)
            }
            ModelAttemptOutcome::Exhausted { attempts } => {
                hooks.empty_model_exhausted(round, attempts).await?;
                Err(anyhow::anyhow!(
                    "model returned empty output {attempts} consecutive times"
                ))
            }
            ModelAttemptOutcome::Accepted if !output.tool_calls.is_empty() => {
                hooks.handle_output(round, output, None, self).await
            }
            ModelAttemptOutcome::Accepted => {
                let directive = self.directive(&output.content);
                hooks
                    .handle_output(round, output, Some(directive), self)
                    .await
            }
        }
    }

    pub fn start_request(&mut self) -> Result<(), AgentLoopStop> {
        self.orchestrator.start_request()
    }

    pub fn record_model_error(&mut self, transient: bool) -> ModelAttemptOutcome {
        self.orchestrator.record_model_error(transient)
    }

    pub fn record_model_output(
        &mut self,
        input_tokens: u64,
        output_tokens: u64,
        empty: bool,
    ) -> ModelAttemptOutcome {
        self.orchestrator
            .record_model_output(input_tokens, output_tokens, empty)
    }

    pub fn record_tool_result(&mut self, success: bool, effective: bool) {
        let (success, effective) = self.config.observation_policy.project(success, effective);
        self.orchestrator.record_tool_result(success, effective);
    }

    pub fn directive(&self, content: &str) -> AgentDirective {
        self.orchestrator.directive(content)
    }

    pub fn completion_rejection(&self, directive: &AgentDirective) -> Option<CompletionRejection> {
        self.config
            .completion_policy
            .rejection(directive, self.checkpoint())
    }

    pub fn finish(&mut self) {
        self.orchestrator.finish();
    }

    fn run_result<T>(&self, output: Option<T>) -> AgentRunResult<T> {
        AgentRunResult {
            output,
            checkpoint: self.checkpoint(),
            stop_reason: self.stop_reason(),
            finished_normally: self.finished_normally(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> AgentLoopLimits {
        AgentLoopLimits {
            max_rounds: 2,
            max_duration_seconds: 60,
            max_requests: 3,
            max_tokens: 100,
            stall_limit: None,
        }
    }

    #[test]
    fn runner_advances_rounds_without_adapter_owned_indexes() {
        let mut runner = AgentRunner::new(AgentRunConfig::main(limits(), 2));
        assert_eq!(runner.begin_next_round(), Ok(0));
        assert_eq!(runner.begin_next_round(), Ok(1));
        assert_eq!(runner.begin_next_round(), Err(AgentLoopStop::Rounds));
    }

    #[test]
    fn sub_agent_completion_requires_an_effective_tool_observation() {
        let mut runner = AgentRunner::new(AgentRunConfig::sub_agent(limits()));
        let directive = AgentDirective::Final("done".into());
        assert_eq!(
            runner.completion_rejection(&directive),
            Some(CompletionRejection::InsufficientToolCalls {
                required: 1,
                actual: 0,
            })
        );
        runner.record_tool_result(false, true);
        assert_eq!(
            runner.completion_rejection(&directive),
            Some(CompletionRejection::NoEffectiveObservation)
        );
        runner.record_tool_result(true, true);
        assert_eq!(runner.completion_rejection(&directive), None);
    }

    struct TwoRoundHook;

    #[async_trait::async_trait]
    impl AgentRunHooks for TwoRoundHook {
        type Output = usize;

        async fn run_round(
            &mut self,
            round: usize,
            _runner: &mut AgentRunner,
        ) -> anyhow::Result<AgentRoundOutcome<Self::Output>> {
            Ok(if round == 1 {
                AgentRoundOutcome::Complete(round)
            } else {
                AgentRoundOutcome::Continue
            })
        }
    }

    #[tokio::test]
    async fn runtime_drives_round_hooks_to_normal_completion() {
        let mut runner = AgentRunner::new(AgentRunConfig::main(limits(), 2));
        let result = runner.run(&mut TwoRoundHook).await.unwrap();
        assert_eq!(result.output, Some(1));
        assert_eq!(result.checkpoint.next_round, 2);
        assert!(result.finished_normally);
    }

    struct ExhaustedHook;

    #[async_trait::async_trait]
    impl AgentRunHooks for ExhaustedHook {
        type Output = AgentLoopStop;

        async fn run_round(
            &mut self,
            _round: usize,
            _runner: &mut AgentRunner,
        ) -> anyhow::Result<AgentRoundOutcome<Self::Output>> {
            Ok(AgentRoundOutcome::Continue)
        }

        async fn stopped(
            &mut self,
            reason: AgentLoopStop,
            _checkpoint: AgentLoopCheckpoint,
            _runner: &AgentRunner,
        ) -> anyhow::Result<Option<Self::Output>> {
            Ok(Some(reason))
        }
    }

    #[tokio::test]
    async fn runtime_owns_budget_stop_and_calls_stop_hook() {
        let mut runner = AgentRunner::new(AgentRunConfig::sub_agent(limits()));
        let result = runner.run(&mut ExhaustedHook).await.unwrap();
        assert_eq!(result.output, Some(AgentLoopStop::Rounds));
        assert_eq!(result.stop_reason, Some(AgentLoopStop::Rounds));
        assert!(!result.finished_normally);
    }

    struct ModelRoundHook {
        directive: Option<AgentDirective>,
    }

    #[async_trait::async_trait]
    impl AgentRoundHooks for ModelRoundHook {
        type Output = String;

        async fn generate(&mut self, _round: usize) -> anyhow::Result<ModelTurnOutput> {
            Ok(ModelTurnOutput {
                content: "FINAL: verified".into(),
                reasoning_summary: None,
                plan_update: None,
                citations: Vec::new(),
                usage: kairos_platform_pent_api::ModelUsage {
                    input_tokens: 7,
                    output_tokens: 3,
                    ..Default::default()
                },
                tool_calls: Vec::new(),
            })
        }

        async fn handle_output(
            &mut self,
            _round: usize,
            output: ModelTurnOutput,
            directive: Option<AgentDirective>,
            _runner: &mut AgentRunner,
        ) -> anyhow::Result<AgentRoundOutcome<Self::Output>> {
            self.directive = directive;
            Ok(AgentRoundOutcome::Complete(output.content))
        }
    }

    #[tokio::test]
    async fn model_round_pipeline_records_usage_and_routes_directive() {
        let mut runner = AgentRunner::new(AgentRunConfig::main(limits(), 2));
        let mut hook = ModelRoundHook { directive: None };
        let outcome = runner.run_model_round(0, &mut hook).await.unwrap();

        assert!(matches!(outcome, AgentRoundOutcome::Complete(_)));
        assert_eq!(runner.checkpoint().token_count, 10);
        assert_eq!(
            hook.directive,
            Some(AgentDirective::Final("verified".into()))
        );
    }
}
