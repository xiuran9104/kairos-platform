//! Quality feedback & RMOS (AI-09).
//! Collects execution results, vulnerability confirmations, human feedback.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFeedback {
    pub tool_id: String,
    pub version: String,
    pub run_id: String,
    pub execution_success: bool,
    pub result_effective: bool,
    pub vulnerability_confirmed: Option<bool>,
    pub false_positive: bool,
    pub human_rating: Option<i32>, // 1-5
    pub duration_ms: u64,
    pub recorded_at: DateTime<Utc>,
}

impl ToolFeedback {
    pub fn new(tool_id: &str, version: &str, run_id: &str, success: bool) -> Self {
        Self {
            tool_id: tool_id.to_string(),
            version: version.to_string(),
            run_id: run_id.to_string(),
            execution_success: success,
            result_effective: success,
            vulnerability_confirmed: None,
            false_positive: false,
            human_rating: None,
            duration_ms: 0,
            recorded_at: Utc::now(),
        }
    }

    pub fn with_human_feedback(mut self, rating: i32, confirmed: bool, false_pos: bool) -> Self {
        self.human_rating = Some(rating);
        self.vulnerability_confirmed = Some(confirmed);
        self.false_positive = false_pos;
        self
    }
}

/// Aggregated quality metrics per tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolQualityMetrics {
    pub tool_id: String,
    pub version: String,
    pub total_executions: u64,
    pub execution_success_rate: f64,
    pub result_effectiveness_rate: f64,
    pub vulnerability_confirmation_rate: f64,
    pub false_positive_rate: f64,
    pub avg_human_rating: f64,
    pub avg_duration_ms: u64,
    pub sample_size: u64,
    pub updated_at: DateTime<Utc>,
}

impl Default for ToolQualityMetrics {
    fn default() -> Self {
        Self {
            tool_id: String::new(),
            version: String::new(),
            total_executions: 0,
            execution_success_rate: 0.0,
            result_effectiveness_rate: 0.0,
            vulnerability_confirmation_rate: 0.0,
            false_positive_rate: 0.0,
            avg_human_rating: 0.0,
            avg_duration_ms: 0,
            sample_size: 0,
            updated_at: Utc::now(),
        }
    }
}

/// Compute quality metrics from feedback history.
pub fn compute_quality_metrics(feedback_history: &[ToolFeedback]) -> ToolQualityMetrics {
    if feedback_history.is_empty() {
        return ToolQualityMetrics::default();
    }
    let n = feedback_history.len() as f64;
    let success = feedback_history
        .iter()
        .filter(|f| f.execution_success)
        .count() as f64
        / n;
    let effective = feedback_history
        .iter()
        .filter(|f| f.result_effective)
        .count() as f64
        / n;
    let confirmed = feedback_history
        .iter()
        .filter(|f| f.vulnerability_confirmed == Some(true))
        .count() as f64
        / n.max(1.0);
    let fp = feedback_history.iter().filter(|f| f.false_positive).count() as f64 / n.max(1.0);
    let avg_rating = feedback_history
        .iter()
        .filter_map(|f| f.human_rating)
        .sum::<i32>() as f64
        / feedback_history
            .iter()
            .filter(|f| f.human_rating.is_some())
            .count()
            .max(1) as f64;

    ToolQualityMetrics {
        tool_id: feedback_history[0].tool_id.clone(),
        version: feedback_history[0].version.clone(),
        total_executions: feedback_history.len() as u64,
        execution_success_rate: success,
        result_effectiveness_rate: effective,
        vulnerability_confirmation_rate: confirmed,
        false_positive_rate: fp,
        avg_human_rating: avg_rating,
        avg_duration_ms: feedback_history.iter().map(|f| f.duration_ms).sum::<u64>()
            / feedback_history.len().max(1) as u64,
        sample_size: feedback_history.len() as u64,
        updated_at: Utc::now(),
    }
}
