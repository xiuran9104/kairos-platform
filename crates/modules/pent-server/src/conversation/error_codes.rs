//! Standard Pent conversation error codes.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

pub const MODEL_UNAVAILABLE: &str = "MODEL_UNAVAILABLE";
pub const MODEL_TIMEOUT: &str = "MODEL_TIMEOUT";
pub const MODEL_OUTPUT_INVALID: &str = "MODEL_OUTPUT_INVALID";
pub const CONTEXT_TOO_LARGE: &str = "CONTEXT_TOO_LARGE";
pub const SCOPE_NOT_CONFIRMED: &str = "SCOPE_NOT_CONFIRMED";
pub const RUN_NOT_ACTIVE: &str = "RUN_NOT_ACTIVE";
pub const INSTRUCTION_REJECTED: &str = "INSTRUCTION_REJECTED";
pub const POLICY_DENIED: &str = "POLICY_DENIED";
pub const BUDGET_EXHAUSTED: &str = "BUDGET_EXHAUSTED";
pub const STREAM_INTERRUPTED: &str = "STREAM_INTERRUPTED";
pub const TURN_CANCELLED: &str = "TURN_CANCELLED";
pub const PLAN_VERSION_CONFLICT: &str = "PLAN_VERSION_CONFLICT";

impl TurnError {
    pub fn new(code: &'static str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }

    pub fn unavailable(msg: impl Into<String>) -> Self {
        Self::new(MODEL_UNAVAILABLE, msg, true)
    }
    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::new(MODEL_TIMEOUT, msg, true)
    }
    pub fn invalid_output(msg: impl Into<String>) -> Self {
        Self::new(MODEL_OUTPUT_INVALID, msg, false)
    }
    pub fn context_too_large(msg: impl Into<String>) -> Self {
        Self::new(CONTEXT_TOO_LARGE, msg, false)
    }
    pub fn scope_not_confirmed(msg: impl Into<String>) -> Self {
        Self::new(SCOPE_NOT_CONFIRMED, msg, false)
    }
    pub fn policy_denied(msg: impl Into<String>) -> Self {
        Self::new(POLICY_DENIED, msg, false)
    }
    pub fn cancelled() -> Self {
        Self::new(TURN_CANCELLED, "turn cancelled by user", false)
    }
    pub fn budget_exhausted() -> Self {
        Self::new(BUDGET_EXHAUSTED, "token or cost budget exhausted", false)
    }
}
