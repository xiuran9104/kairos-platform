//! Unified execution routing across builtin, MCP, and HTTP adapters.

use std::sync::Arc;

use async_trait::async_trait;
use kairos_platform_pent_api::{ToolDescriptor, ToolExecutionResult, ToolInvocation, ToolSource};

use crate::ports::ToolExecutionBackend;

#[derive(Default)]
pub struct ToolExecutorRouter {
    builtin: Option<Arc<dyn ToolExecutionBackend>>,
    mcp: Option<Arc<dyn ToolExecutionBackend>>,
    http: Option<Arc<dyn ToolExecutionBackend>>,
}

impl ToolExecutorRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_backend(
        mut self,
        source: ToolSource,
        backend: Arc<dyn ToolExecutionBackend>,
    ) -> Self {
        self.set_backend(source, backend);
        self
    }

    pub fn set_backend(&mut self, source: ToolSource, backend: Arc<dyn ToolExecutionBackend>) {
        match source {
            ToolSource::Builtin => self.builtin = Some(backend),
            ToolSource::Mcp => self.mcp = Some(backend),
            ToolSource::Http => self.http = Some(backend),
        }
    }

    fn backend(&self, source: &ToolSource) -> anyhow::Result<&Arc<dyn ToolExecutionBackend>> {
        match source {
            ToolSource::Builtin => self.builtin.as_ref(),
            ToolSource::Mcp => self.mcp.as_ref(),
            ToolSource::Http => self.http.as_ref(),
        }
        .ok_or_else(|| anyhow::anyhow!("工具执行后端未注册: {source:?}"))
    }
}

#[async_trait]
impl ToolExecutionBackend for ToolExecutorRouter {
    async fn execute(
        &self,
        descriptor: &ToolDescriptor,
        invocation: &ToolInvocation,
    ) -> anyhow::Result<ToolExecutionResult> {
        self.backend(&descriptor.executor.kind)?
            .execute(descriptor, invocation)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UnusedBackend;

    #[async_trait]
    impl ToolExecutionBackend for UnusedBackend {
        async fn execute(
            &self,
            _descriptor: &ToolDescriptor,
            _invocation: &ToolInvocation,
        ) -> anyhow::Result<ToolExecutionResult> {
            unreachable!("backend execution is not needed by routing tests")
        }
    }

    #[test]
    fn reports_unregistered_source() {
        let router = ToolExecutorRouter::new();
        assert!(router.backend(&ToolSource::Mcp).is_err());
    }

    #[test]
    fn registers_backends_independently_by_source() {
        let router = ToolExecutorRouter::new()
            .with_backend(ToolSource::Builtin, Arc::new(UnusedBackend))
            .with_backend(ToolSource::Mcp, Arc::new(UnusedBackend));
        assert!(router.backend(&ToolSource::Builtin).is_ok());
        assert!(router.backend(&ToolSource::Mcp).is_ok());
        assert!(router.backend(&ToolSource::Http).is_err());
    }
}
