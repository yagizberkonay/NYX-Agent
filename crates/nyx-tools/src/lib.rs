use async_trait::async_trait;
use nyx_core::{ExecutionTarget, PermissionClass};
use nyx_security::PolicyDecision;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub version: String,
    pub description: String,
    pub permission: PermissionClass,
    pub target: ExecutionTarget,
    pub timeout_ms: u64,
    pub idempotent: bool,
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub task_id: Uuid,
    pub invocation_id: Uuid,
    pub workspace_root: std::path::PathBuf,
    pub approved: bool,
    pub target: ExecutionTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub invocation_id: Uuid,
    pub tool: String,
    pub success: bool,
    pub summary: String,
    pub data: Value,
    pub duration_ms: u64,
    pub redacted: bool,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("input validation failed: {0}")]
    InvalidInput(String),
    #[error("permission denied")]
    PermissionDenied,
    #[error("tool execution failed: {0}")]
    Execution(String),
    #[error("tool timed out")]
    Timeout,
    #[error("tool cancelled")]
    Cancelled,
}

#[async_trait]
pub trait NyxTool: Send + Sync {
    fn descriptor(&self) -> ToolDescriptor;
    async fn validate(&self, input: Value, context: &ToolContext) -> Result<Value, ToolError>;
    async fn execute(
        &self,
        input: Value,
        context: ToolContext,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, ToolError>;
}

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: Arc<HashMap<String, Arc<dyn NyxTool>>>,
}

impl ToolRegistry {
    pub fn new(tools: Vec<Arc<dyn NyxTool>>) -> Self {
        let mut indexed = HashMap::new();
        for tool in tools {
            indexed.insert(tool.descriptor().name, tool);
        }
        Self {
            tools: Arc::new(indexed),
        }
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        let mut descriptors: Vec<_> = self.tools.values().map(|tool| tool.descriptor()).collect();
        descriptors.sort_by(|left, right| left.name.cmp(&right.name));
        descriptors
    }

    pub fn get(&self, name: &str) -> Result<Arc<dyn NyxTool>, ToolError> {
        self.tools
            .get(name)
            .cloned()
            .ok_or_else(|| ToolError::NotFound(name.to_owned()))
    }

    pub fn permission_for(&self, name: &str) -> Result<PolicyDecision, ToolError> {
        let descriptor = self.get(name)?.descriptor();
        Ok(match descriptor.permission {
            PermissionClass::Allow => PolicyDecision::Allow,
            PermissionClass::Ask => PolicyDecision::Ask,
            PermissionClass::Deny => PolicyDecision::Deny,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool;

    #[async_trait]
    impl NyxTool for EchoTool {
        fn descriptor(&self) -> ToolDescriptor {
            ToolDescriptor {
                name: "echo".into(),
                version: "1".into(),
                description: "Echo input".into(),
                permission: PermissionClass::Allow,
                target: ExecutionTarget::Host,
                timeout_ms: 1000,
                idempotent: true,
                input_schema: serde_json::json!({"type": "object"}),
            }
        }

        async fn validate(&self, input: Value, _context: &ToolContext) -> Result<Value, ToolError> {
            Ok(input)
        }

        async fn execute(
            &self,
            input: Value,
            context: ToolContext,
            _cancellation: CancellationToken,
        ) -> Result<ToolResult, ToolError> {
            Ok(ToolResult {
                invocation_id: context.invocation_id,
                tool: "echo".into(),
                success: true,
                summary: "Echoed".into(),
                data: input,
                duration_ms: 0,
                redacted: false,
            })
        }
    }

    #[test]
    fn registry_exposes_sorted_descriptors() {
        let registry = ToolRegistry::new(vec![Arc::new(EchoTool)]);
        assert_eq!(registry.descriptors()[0].name, "echo");
        assert_eq!(
            registry.permission_for("echo").unwrap(),
            PolicyDecision::Allow
        );
    }
}
