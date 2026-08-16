//! Shared state machine for top-level and sub-agent tool loops.
//!
//! The runtime-specific code still owns model calls, HITL, tool dispatch and
//! persistence. This module owns the counters and terminal budget decisions so
//! every kind of agent follows the same accounting rules.

use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentDirective {
    Final(String),
    Replan(String),
    NoPath(String),
    AskUser(String),
    Candidate(String),
}

/// Parse the text or JSON lifecycle protocol shared by every agent loop.
pub(crate) fn parse_agent_directive(content: &str) -> AgentDirective {
    let content = content.trim();

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content)
        && let Some(action) = parsed.get("action").and_then(serde_json::Value::as_str)
    {
        let body = parsed
            .get("content")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim();
        let value = if body.is_empty() { content } else { body }.to_string();
        return match action.to_ascii_lowercase().as_str() {
            "final" => AgentDirective::Final(value),
            "replan" => AgentDirective::Replan(value),
            "no_path" => AgentDirective::NoPath(value),
            "ask_user" => AgentDirective::AskUser(value),
            _ => AgentDirective::Candidate(content.to_string()),
        };
    }

    for marker in ["FINAL:", "Final:", "final:", "最终结果：", "完成："] {
        if let Some(value) = content.strip_prefix(marker) {
            return AgentDirective::Final(value.trim().to_string());
        }
    }
    for marker in ["REPLAN:", "Replan:", "replan:", "重新规划："] {
        if let Some(value) = content.strip_prefix(marker) {
            return AgentDirective::Replan(value.trim().to_string());
        }
    }
    for marker in [
        "NO_PATH:",
        "No viable path:",
        "no_path:",
        "无法继续：",
        "没有可继续验证的路径：",
    ] {
        if let Some(value) = content.strip_prefix(marker) {
            return AgentDirective::NoPath(value.trim().to_string());
        }
    }
    for marker in [
        "ASK_USER:",
        "Ask user:",
        "ask_user:",
        "需要用户：",
        "请用户确认：",
    ] {
        if let Some(value) = content.strip_prefix(marker) {
            return AgentDirective::AskUser(value.trim().to_string());
        }
    }

    AgentDirective::Candidate(content.to_string())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AgentLoopLimits {
    pub max_rounds: usize,
    pub max_duration_seconds: u64,
    pub max_requests: usize,
    pub max_tokens: u64,
    pub stall_limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AgentLoopCheckpoint {
    pub next_round: usize,
    pub request_count: usize,
    pub token_count: u64,
    pub tool_call_count: usize,
    pub effective_observations: usize,
    pub consecutive_failures: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentLoopStop {
    Rounds,
    Time,
    Requests,
    Tokens,
    Stalled,
}

impl AgentLoopStop {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Rounds => "rounds",
            Self::Time => "time",
            Self::Requests => "requests",
            Self::Tokens => "tokens",
            Self::Stalled => "stalled",
        }
    }
}

#[derive(Debug)]
pub(crate) struct AgentLoopEngine {
    limits: AgentLoopLimits,
    state: AgentLoopCheckpoint,
    started_at: Instant,
    rounds_used: usize,
}

impl AgentLoopEngine {
    pub(crate) fn new(limits: AgentLoopLimits) -> Self {
        Self::from_checkpoint(limits, AgentLoopCheckpoint::default())
    }

    pub(crate) fn from_checkpoint(
        limits: AgentLoopLimits,
        checkpoint: AgentLoopCheckpoint,
    ) -> Self {
        Self {
            limits,
            state: checkpoint,
            started_at: Instant::now(),
            rounds_used: checkpoint.next_round,
        }
    }

    pub(crate) const fn limits(&self) -> &AgentLoopLimits {
        &self.limits
    }

    pub(crate) const fn checkpoint(&self) -> AgentLoopCheckpoint {
        self.state
    }

    pub(crate) const fn next_round(&self) -> usize {
        self.state.next_round
    }

    pub(crate) const fn rounds_used(&self) -> usize {
        self.rounds_used
    }

    pub(crate) fn begin_round(&mut self, round: usize) -> Result<(), AgentLoopStop> {
        if round >= self.limits.max_rounds {
            return Err(AgentLoopStop::Rounds);
        }
        if self.started_at.elapsed().as_secs() >= self.limits.max_duration_seconds {
            return Err(AgentLoopStop::Time);
        }
        if self.state.token_count >= self.limits.max_tokens {
            return Err(AgentLoopStop::Tokens);
        }
        if self.state.request_count >= self.limits.max_requests {
            return Err(AgentLoopStop::Requests);
        }
        if self
            .limits
            .stall_limit
            .is_some_and(|limit| self.state.consecutive_failures >= limit)
        {
            return Err(AgentLoopStop::Stalled);
        }
        self.rounds_used = round.saturating_add(1);
        self.state.next_round = round.saturating_add(1);
        Ok(())
    }

    pub(crate) fn start_request(&mut self) -> Result<(), AgentLoopStop> {
        if self.state.request_count >= self.limits.max_requests {
            return Err(AgentLoopStop::Requests);
        }
        self.state.request_count = self.state.request_count.saturating_add(1);
        Ok(())
    }

    pub(crate) fn record_model_usage(&mut self, input_tokens: u64, output_tokens: u64) {
        self.state.token_count = self
            .state
            .token_count
            .saturating_add(input_tokens)
            .saturating_add(output_tokens);
    }

    pub(crate) fn record_tool_result(&mut self, success: bool, effective: bool) {
        self.state.tool_call_count = self.state.tool_call_count.saturating_add(1);
        if success && effective {
            self.state.effective_observations = self.state.effective_observations.saturating_add(1);
            self.state.consecutive_failures = 0;
        } else {
            self.state.consecutive_failures = self.state.consecutive_failures.saturating_add(1);
        }
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
            stall_limit: Some(2),
        }
    }

    #[test]
    fn restores_checkpoint_and_accounts_requests_and_tokens() {
        let mut engine = AgentLoopEngine::from_checkpoint(
            limits(),
            AgentLoopCheckpoint {
                next_round: 1,
                request_count: 1,
                token_count: 20,
                effective_observations: 1,
                ..Default::default()
            },
        );
        assert_eq!(engine.next_round(), 1);
        engine.begin_round(1).unwrap();
        engine.start_request().unwrap();
        engine.record_model_usage(30, 10);
        assert_eq!(engine.checkpoint().request_count, 2);
        assert_eq!(engine.checkpoint().token_count, 60);
        assert_eq!(engine.begin_round(2), Err(AgentLoopStop::Requests));
    }

    #[test]
    fn successful_observation_resets_stall_counter() {
        let mut engine = AgentLoopEngine::new(limits());
        engine.record_tool_result(false, false);
        engine.record_tool_result(false, false);
        assert_eq!(engine.begin_round(0), Err(AgentLoopStop::Stalled));

        engine.record_tool_result(true, true);
        assert_eq!(engine.checkpoint().effective_observations, 1);
        assert_eq!(engine.checkpoint().consecutive_failures, 0);
        engine.begin_round(0).unwrap();
    }

    #[test]
    fn lifecycle_protocol_is_shared_across_agent_types() {
        assert_eq!(
            parse_agent_directive(r#"{"action":"FINAL","content":"done"}"#),
            AgentDirective::Final("done".into())
        );
        assert_eq!(
            parse_agent_directive("请用户确认：提供测试账号"),
            AgentDirective::AskUser("提供测试账号".into())
        );
        assert_eq!(
            parse_agent_directive("普通阶段性结论"),
            AgentDirective::Candidate("普通阶段性结论".into())
        );
    }
}
