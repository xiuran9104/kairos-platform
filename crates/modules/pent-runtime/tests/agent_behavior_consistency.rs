use kairos_platform_pent_runtime::agent_loop::{
    AgentDirective, AgentLoopCheckpoint, AgentLoopLimits, AgentLoopStop,
};
use kairos_platform_pent_runtime::agent_runner::{
    AgentRoundOutcome, AgentRunConfig, AgentRunHooks, AgentRunner, CompletionRejection,
};

fn limits() -> AgentLoopLimits {
    AgentLoopLimits {
        max_rounds: 3,
        max_duration_seconds: 60,
        max_requests: 4,
        max_tokens: 100,
        stall_limit: Some(2),
    }
}

fn apply_same_observations(runner: &mut AgentRunner) {
    assert_eq!(runner.begin_next_round(), Ok(0));
    assert_eq!(runner.start_request(), Ok(()));
    runner.record_model_output(7, 3, false);
    runner.record_tool_result(false, true);
    runner.record_tool_result(true, true);
}

#[test]
fn main_and_sub_agents_share_directives_and_budget_accounting() {
    let mut main = AgentRunner::new(AgentRunConfig::main(limits(), 2));
    let mut child = AgentRunner::new(AgentRunConfig::sub_agent(limits()));

    for input in [
        "FINAL: verified",
        "REPLAN: enumerate subdomains",
        "NO_PATH: target unreachable",
        "ASK_USER: provide scope",
        "candidate result",
    ] {
        assert_eq!(main.directive(input), child.directive(input));
    }

    apply_same_observations(&mut main);
    apply_same_observations(&mut child);
    assert_eq!(main.checkpoint(), child.checkpoint());
    assert_eq!(
        main.checkpoint(),
        AgentLoopCheckpoint {
            next_round: 1,
            request_count: 1,
            token_count: 10,
            tool_call_count: 2,
            effective_observations: 1,
            consecutive_failures: 0,
        }
    );

    let final_directive = AgentDirective::Final("verified".into());
    assert_eq!(main.completion_rejection(&final_directive), None);
    assert_eq!(child.completion_rejection(&final_directive), None);
}

#[test]
fn intentional_main_sub_policy_differences_remain_explicit() {
    let main = AgentRunner::new(AgentRunConfig::main(limits(), 2));
    let child = AgentRunner::new(AgentRunConfig::sub_agent(limits()));
    let final_directive = AgentDirective::Final("premature".into());

    assert!(main.config().allow_sub_agents);
    assert!(main.config().emit_turn_events);
    assert!(!child.config().allow_sub_agents);
    assert!(!child.config().emit_turn_events);
    assert_eq!(main.completion_rejection(&final_directive), None);
    assert_eq!(
        child.completion_rejection(&final_directive),
        Some(CompletionRejection::InsufficientToolCalls {
            required: 1,
            actual: 0,
        })
    );
}

struct ExhaustionHook;

#[async_trait::async_trait]
impl AgentRunHooks for ExhaustionHook {
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
async fn main_and_sub_agents_stop_at_the_same_runtime_budget_boundary() {
    let mut main = AgentRunner::new(AgentRunConfig::main(limits(), 2));
    let mut child = AgentRunner::new(AgentRunConfig::sub_agent(limits()));

    let main_result = main.run(&mut ExhaustionHook).await.unwrap();
    let child_result = child.run(&mut ExhaustionHook).await.unwrap();

    assert_eq!(main_result.stop_reason, Some(AgentLoopStop::Rounds));
    assert_eq!(main_result.stop_reason, child_result.stop_reason);
    assert_eq!(main_result.checkpoint, child_result.checkpoint);
    assert_eq!(main_result.output, child_result.output);
}
