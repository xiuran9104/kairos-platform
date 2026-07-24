//! Scan task management — ported from rustset.
//!
//! Task lifecycle: pending → running → completed/failed.

use chrono::Utc;
use kairos_platform_scan_api::{ScanTask, TaskStatus};

/// Transition a task to running state.
pub fn start_task(task: &mut ScanTask) {
    task.status = TaskStatus::Running;
    task.start_time = Some(Utc::now().to_rfc3339());
}

/// Transition a task to completed state with result counts.
pub fn complete_task(task: &mut ScanTask, found_assets: usize, found_risks: usize) {
    task.status = TaskStatus::Completed;
    task.end_time = Some(Utc::now().to_rfc3339());
    task.found_assets = found_assets;
    task.found_risks = found_risks;
}

/// Transition a task to failed state with an error message.
#[allow(dead_code)]
pub fn fail_task(task: &mut ScanTask, _error: &str) {
    task.status = TaskStatus::Failed;
    task.end_time = Some(Utc::now().to_rfc3339());
}

/// Check if a task's target string is valid (not empty, reasonable length).
pub fn validate_task_target(target: &str) -> Result<(), String> {
    if target.trim().is_empty() {
        return Err("target is required".into());
    }
    if target.len() > 1000 {
        return Err("target is too long (max 1000 chars)".into());
    }
    Ok(())
}

/// Parse a comma-separated or whitespace-separated list of IPs/CIDRs from a target string.
pub fn parse_targets(target: &str) -> Vec<String> {
    target
        .split(&[',', ' ', '\n', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_lifecycle() {
        let mut task = ScanTask {
            id: "test-1".into(),
            name: "Test".into(),
            target: "127.0.0.1".into(),
            status: TaskStatus::Pending,
            start_time: None,
            end_time: None,
            found_assets: 0,
            found_risks: 0,
            port_policy: "TOP100".into(),
            domain_brute: false,
            service_detection: true,
            os_detection: false,
            site_identify: false,
        };

        assert!(matches!(task.status, TaskStatus::Pending));
        start_task(&mut task);
        assert!(matches!(task.status, TaskStatus::Running));
        assert!(task.start_time.is_some());
        complete_task(&mut task, 3, 1);
        assert!(matches!(task.status, TaskStatus::Completed));
        assert_eq!(task.found_assets, 3);
        assert_eq!(task.found_risks, 1);
    }

    #[test]
    fn test_parse_targets() {
        let targets = parse_targets("192.168.1.1, 10.0.0.1, 172.16.0.0/24");
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0], "192.168.1.1");
    }

    #[test]
    fn test_validate_target() {
        assert!(validate_task_target("").is_err());
        assert!(validate_task_target("  ").is_err());
        assert!(validate_task_target("127.0.0.1").is_ok());
        assert!(validate_task_target(&"x".repeat(1001)).is_err());
    }
}
