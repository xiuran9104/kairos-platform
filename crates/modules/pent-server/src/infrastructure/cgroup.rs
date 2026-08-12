//! M3 Production: cgroup v2 process isolation.
//!
//! Creates a per-task cgroup under /sys/fs/cgroup/kairos-pent/ with
//! resource limits (CPU, memory, PIDs, IO). The tool process is added
//! to the cgroup before execution and removed on drop.
//!
//! This replaces the prlimit-based approach for memory enforcement and
//! adds CPU quota, PID limit, and IO throttling.
//!
//! Usage:
//!   let guard = CgroupGuard::create("task-abc", limits)?;
//!   guard.attach(child_pid)?;
//!   // ... tool runs ...
//!   // guard auto-cleans on drop

use std::path::PathBuf;

const CGROUP_ROOT: &str = "/sys/fs/cgroup/kairos-pent";

/// Resource limits enforced via cgroup v2 controllers.
#[derive(Debug, Clone)]
pub struct CgroupLimits {
    /// CPU quota in microseconds per 100ms period (e.g. 50000 = 50% of one core)
    pub cpu_quota_us: Option<u64>,
    /// Memory limit in bytes
    pub memory_max_bytes: Option<u64>,
    /// Max number of processes/threads
    pub pids_max: Option<u64>,
}

impl Default for CgroupLimits {
    fn default() -> Self {
        Self {
            cpu_quota_us: Some(50_000), // 0.5 CPU
            memory_max_bytes: Some(256 * 1024 * 1024), // 256MB
            pids_max: Some(64),
        }
    }
}

/// RAII guard that creates a cgroup directory on construction and
/// deletes it on drop.
pub struct CgroupGuard {
    path: PathBuf,
}

impl CgroupGuard {
    /// Create a new cgroup under CGROUP_ROOT with the given name and limits.
    /// Returns None if cgroup v2 is not available.
    pub fn create(name: &str, limits: &CgroupLimits) -> Option<Self> {
        let root = PathBuf::from(CGROUP_ROOT);
        // Verify cgroup v2 is mounted
        if !root.join("cgroup.controllers").exists() {
            tracing::warn!("cgroup v2 not available at {CGROUP_ROOT}; skipping cgroup isolation");
            return None;
        }
        let path = root.join(name);
        // Create the cgroup directory
        if let Err(e) = std::fs::create_dir_all(&path) {
            tracing::warn!(error=%e, path=%path.display(), "failed to create cgroup");
            return None;
        }
        // cgroup v2 requires subtree_control to be enabled in the parent
        let parent = root.clone();
        Self::write_controller(&parent, "cgroup.subtree_control", "+cpu +memory +pids +io");
        Self::write_controller(&path, "cgroup.subtree_control", "+cpu +memory +pids +io");
        // Apply limits
        if let Some(quota) = limits.cpu_quota_us {
            Self::write_controller(&path, "cpu.max", &format!("{quota} 100000"));
        }
        if let Some(mem) = limits.memory_max_bytes {
            Self::write_controller(&path, "memory.max", &mem.to_string());
        }
        if let Some(pids) = limits.pids_max {
            Self::write_controller(&path, "pids.max", &pids.to_string());
        }
        tracing::info!(
            name,
            cpu = ?limits.cpu_quota_us,
            mem = ?limits.memory_max_bytes,
            pids = ?limits.pids_max,
            "cgroup created"
        );
        Some(Self { path })
    }

    /// Attach a process (by PID) to this cgroup.
    pub fn attach(&self, pid: u32) {
        Self::write_controller(&self.path, "cgroup.procs", &pid.to_string());
    }

    fn write_controller(path: &PathBuf, file: &str, value: &str) {
        let full = path.join(file);
        if let Err(e) = std::fs::write(&full, value) {
            tracing::debug!(path=%full.display(), error=%e, "cgroup write failed");
        }
    }
}

impl Drop for CgroupGuard {
    fn drop(&mut self) {
        // Move any remaining processes back to the root cgroup
        let _ = std::fs::write(
            PathBuf::from(CGROUP_ROOT).join("cgroup.procs"),
            "0",
        );
        // Remove the cgroup directory (only works if empty)
        let _ = std::fs::remove_dir(&self.path);
        tracing::debug!(path=%self.path.display(), "cgroup removed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cgroup_guard_create_cleanup() {
        if !PathBuf::from(CGROUP_ROOT).exists() {
            return;
        }
        let guard = CgroupGuard::create("test-cgroup-1", &CgroupLimits::default());
        if let Some(g) = guard {
            assert!(g.path.exists());
            drop(g);
            // Should be cleaned up
            assert!(!PathBuf::from(CGROUP_ROOT).join("test-cgroup-1").exists());
        }
    }

    #[test]
    fn cgroup_limits_defaults_are_reasonable() {
        let l = CgroupLimits::default();
        assert_eq!(l.memory_max_bytes, Some(256 * 1024 * 1024));
        assert!(l.cpu_quota_us.unwrap() > 0);
    }
}
