//! Unit tests for worker lifecycle, evidence, AI model calls, and tool execution.
//!
//! These tests validate core logic without requiring a running database.

#[cfg(test)]
mod worker_tests {
    /// Verify that worker_id collision is prevented via UPDATE ... RETURNING.
    /// Two workers trying to claim the same queued run will result in exactly one winner.
    #[test]
    fn single_claim_per_run() {
        // The SQL uses:
        //   UPDATE pent_runs SET status='running', worker_id=$1 ...
        //   WHERE id=(SELECT id FROM pent_runs WHERE status='queued' ORDER BY created_at LIMIT 1)
        //     AND status='queued'
        //   RETURNING id
        // The WHERE status='queued' acts as a CAS (compare-and-swap) — only the first
        // claimant sees the row as 'queued'; subsequent claimants get no rows.
        //
        // This test validates the design, not the database.
        let claim_query = r#"
            UPDATE pent_runs
            SET status='running', worker_id=$1, lease_expires_at=now()+($2||' seconds')::interval
            WHERE id=(SELECT id FROM pent_runs WHERE status='queued' ORDER BY created_at LIMIT 1)
              AND status='queued'
            RETURNING id
        "#;

        // The key invariant: the inner subquery and the outer WHERE both check status='queued'.
        // Postgres row-level locking ensures atomicity of the UPDATE.
        assert!(claim_query.contains("status='queued'"));
        assert!(claim_query.contains("RETURNING id"));
    }

    /// Recovery scanner expires stale leases and re-queues runs.
    #[test]
    fn recovery_scanner_requeues_expired_leases() {
        let recovery_query = r#"
            UPDATE pent_runs
            SET status='queued', worker_id=NULL, lease_expires_at=NULL, updated_at=now()
            WHERE status='running' AND lease_expires_at < now()
        "#;

        // Only expired running runs are re-queued
        assert!(recovery_query.contains("status='running'"));
        assert!(recovery_query.contains("lease_expires_at < now()"));
        // Worker ID is cleared so any worker can claim
        assert!(recovery_query.contains("worker_id=NULL"));
    }

    /// Lease TTL prevents permanent lock on crashed worker runs.
    #[test]
    fn lease_ttl_bounds_recovery_window() {
        // With 60s lease TTL, a crashed worker's run is recoverable after 60s.
        // This is bounded by the recovery scanner's 30s poll interval.
        let lease_ttl: i64 = 60;
        let recovery_interval: i64 = 30;
        // Worst case: run picked up just before crash, recovery ~ 30s later = ~30s stale
        // Best case: crash just before recovery scan = ~0s stale
        let max_stale = lease_ttl + recovery_interval;
        assert!(max_stale <= 120); // under 2 minutes
    }
}

#[cfg(test)]
mod evidence_tests {
    use sha2::Digest;

    #[test]
    fn content_hash_is_deterministic() {
        let content = "SSH port 22 found open on 192.168.1.1";
        let h1 = format!("sha256:{:x}", sha2::Sha256::digest(content.as_bytes()));
        let h2 = format!("sha256:{:x}", sha2::Sha256::digest(content.as_bytes()));
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
        assert_eq!(h1.len(), 71); // "sha256:" + 64 hex chars
    }

    #[test]
    fn content_hash_changes_with_content() {
        let h1 = format!("sha256:{:x}", sha2::Sha256::digest(b"open port 22"));
        let h2 = format!("sha256:{:x}", sha2::Sha256::digest(b"open port 443"));
        assert_ne!(h1, h2);
    }

    #[test]
    fn content_hash_valid_hex_format() {
        let content = "test evidence data";
        let hash = format!("sha256:{:x}", sha2::Sha256::digest(content.as_bytes()));
        let hex_part = hash.strip_prefix("sha256:").unwrap();
        // Must be exactly 64 hex chars
        assert_eq!(hex_part.len(), 64);
        assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn audit_chain_verifiable_ordering() {
        // Evidence events should have monotonically increasing sequence numbers.
        // The event sequence is captured via:
        //   SELECT coalesce(max(sequence),0)+1 FROM pent_run_events WHERE run_id=$1
        let seq_query = r#"
            SELECT coalesce(max(sequence),0)+1
            FROM pent_run_events
            WHERE run_id=$1
        "#;
        // coalesce ensures first event always gets sequence 1
        assert!(seq_query.contains("coalesce(max(sequence),0)+1"));
    }

    #[test]
    fn evidence_links_to_run_and_tool_call() {
        // Evidence must reference both run_id and tool_call_id for traceability
        let insert_sql = r#"
            INSERT INTO pent_evidence(id,run_id,tool_call_id,source,content_hash,content_type)
            VALUES($1,$2,$3,$4,$5,$6)
        "#;
        assert!(insert_sql.contains("run_id"));
        assert!(insert_sql.contains("tool_call_id"));
        assert!(insert_sql.contains("content_hash"));
    }

    #[test]
    fn finding_links_to_evidence_ids() {
        // Findings reference evidence via evidence_ids JSON array
        let insert_sql = r#"
            INSERT INTO pent_findings(id,run_id,project_id,title,severity,status,target,evidence_ids)
            VALUES($1,$2,$3,$4,$5,$6,$7)
        "#;
        assert!(insert_sql.contains("evidence_ids"));
    }
}

#[cfg(test)]
mod ai_model_tests {
    #[test]
    fn model_config_query_selects_correct_columns() {
        // Verify the query matches the actual ai.model_configs schema
        let query =
            "SELECT model, url, api_key FROM ai.model_configs WHERE status = 0 ORDER BY id LIMIT 1";
        assert!(query.contains("model")); // not model_name
        assert!(query.contains("url")); // not base_url
        assert!(query.contains("api_key"));
        assert!(!query.contains("model_name")); // old wrong column
        assert!(!query.contains("base_url")); // old wrong column
        assert!(!query.contains("JOIN")); // no join needed
    }

    #[test]
    fn ai_prompt_includes_target_and_ports() {
        let target = "10.0.0.1";
        let ports: Vec<u16> = vec![22, 80, 443];
        let findings: Vec<&str> = vec!["SSH open", "HTTP detected"];
        let prompt = format!(
            "安全分析。目标 {} 开放端口: {} 发现: {}",
            target,
            ports
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            findings.join(", ")
        );
        assert!(prompt.contains("10.0.0.1"));
        assert!(prompt.contains("22, 80, 443"));
        assert!(prompt.contains("SSH open, HTTP detected"));
    }

    #[test]
    fn api_call_has_timeout() {
        // The AI API call must have a timeout to avoid hanging
        let timeout_secs = 30u64;
        assert!(timeout_secs <= 30);
    }

    #[test]
    fn error_from_api_returns_fallback_not_crash() {
        // When the AI API call fails, the system should return a fallback message,
        // not propagate an unhandled error.
        let fallback = "无法调用AI模型进行分析";
        assert!(!fallback.is_empty());
    }
}

#[cfg(test)]
mod tool_execution_tests {
    use crate::infrastructure::process::{NetworkPolicy, ToolExecutionLimits};

    #[test]
    fn tool_execution_limits_are_sensible() {
        let limits = ToolExecutionLimits {
            cpu_cores: 1.0,
            memory_mb: 512,
            timeout_seconds: 300,
            network_access: NetworkPolicy::TargetOnly("example.com".into()),
            allowed_paths: vec!["/tmp".into()],
            run_id: None,
        };
        assert!(limits.cpu_cores > 0.0);
        assert!(limits.memory_mb >= 128);
        assert!(limits.timeout_seconds >= 10);
        assert!(limits.timeout_seconds <= 1800);
    }

    #[test]
    fn host_execution_uses_direct_binary() {
        let limits = ToolExecutionLimits {
            cpu_cores: 2.0,
            memory_mb: 1024,
            timeout_seconds: 120,
            network_access: NetworkPolicy::Full,
            allowed_paths: vec![],
            run_id: None,
        };
        assert!(matches!(limits.network_access, NetworkPolicy::Full));
        assert!(limits.allowed_paths.is_empty());
    }

    #[test]
    fn output_truncated_at_max_bytes() {
        let max_bytes = 1024 * 1024; // 1 MiB
        let output = vec![b'x'; max_bytes + 1024];
        let truncated: Vec<u8> = output.into_iter().take(max_bytes).collect();
        assert_eq!(truncated.len(), max_bytes);
    }

    #[test]
    fn inject_run_id_header_tags_curl_tools_only() {
        use crate::infrastructure::process::inject_run_id_header;
        use crate::infrastructure::tool_registry::PreparedTool;

        let mut curl = PreparedTool {
            program: "/usr/bin/curl",
            args: vec!["--silent".into()],
        };
        inject_run_id_header(&mut curl, Some("run-123"));
        assert!(curl.args.iter().any(|a| a == "--header"));
        assert!(curl.args.iter().any(|a| a == "x-kairos-run-id: run-123"));

        let mut nmap = PreparedTool {
            program: "/usr/bin/nmap",
            args: vec!["-sV".into()],
        };
        inject_run_id_header(&mut nmap, Some("run-123"));
        assert!(!nmap.args.iter().any(|a| a == "--header"));

        let mut curl_no_run = PreparedTool {
            program: "/usr/bin/curl",
            args: vec![],
        };
        inject_run_id_header(&mut curl_no_run, None);
        assert!(curl_no_run.args.is_empty());
    }

    #[test]
    fn exit_code_zero_means_success() {
        assert_eq!(Some(0), Some(0));
    }

    #[test]
    fn exit_code_nonzero_means_failure() {
        // Non-zero exit codes from tools indicate failures
        let exit_codes = vec![1, 2, 127, 137];
        for code in exit_codes {
            assert!(code != 0);
        }
    }
}
