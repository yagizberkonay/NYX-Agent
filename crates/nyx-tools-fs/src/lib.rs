use async_trait::async_trait;
use nyx_core::{ExecutionTarget, PermissionClass};
use nyx_security::{Operation, PolicyEngine, WorkspaceScope};
use nyx_tools::{NyxTool, ToolContext, ToolDescriptor, ToolError, ToolResult};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Instant;
use tokio::fs;
use tokio_util::sync::CancellationToken;

pub struct FileSystemTool {
    name: String,
    policy: Operation,
}

impl FileSystemTool {
    pub fn read() -> Self {
        Self {
            name: "file_read".into(),
            policy: Operation::FileRead,
        }
    }

    pub fn search() -> Self {
        Self {
            name: "file_search".into(),
            policy: Operation::FileSearch,
        }
    }

    pub fn list() -> Self {
        Self {
            name: "directory_list".into(),
            policy: Operation::FileSearch,
        }
    }

    pub fn write() -> Self {
        Self {
            name: "file_write".into(),
            policy: Operation::FileWrite,
        }
    }

    fn scope(&self, context: &ToolContext) -> Result<WorkspaceScope, ToolError> {
        WorkspaceScope::new(&context.workspace_root)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }

    fn input_path(input: &Value) -> Result<PathBuf, ToolError> {
        input
            .get("path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| ToolError::InvalidInput("path must be a string".into()))
    }
}

#[async_trait]
impl NyxTool for FileSystemTool {
    fn descriptor(&self) -> ToolDescriptor {
        let (description, permission) = match self.name.as_str() {
            "file_read" => (
                "Read a text file inside the active workspace",
                PermissionClass::Allow,
            ),
            "file_search" => (
                "Search text files inside the active workspace",
                PermissionClass::Allow,
            ),
            "directory_list" => (
                "List entries inside the active workspace",
                PermissionClass::Allow,
            ),
            "file_write" => (
                "Write text to a file inside the active workspace",
                PermissionClass::Ask,
            ),
            _ => ("Filesystem operation", PermissionClass::Ask),
        };
        ToolDescriptor {
            name: self.name.clone(),
            version: "1.0.0".into(),
            description: description.into(),
            permission,
            target: ExecutionTarget::Host,
            timeout_ms: 10_000,
            idempotent: self.name != "file_write",
            input_schema: match self.name.as_str() {
                "file_read" | "directory_list" => json!({
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"]
                }),
                "file_search" => json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "query": {"type": "string"},
                        "max_results": {"type": "integer", "minimum": 1, "maximum": 100}
                    },
                    "required": ["path", "query"]
                }),
                "file_write" => json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "content": {"type": "string"}
                    },
                    "required": ["path", "content"]
                }),
                _ => json!({"type": "object"}),
            },
        }
    }

    async fn validate(&self, input: Value, context: &ToolContext) -> Result<Value, ToolError> {
        let scope = self.scope(context)?;
        let path = Self::input_path(&input)?;
        scope
            .resolve(path)
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        if self.name == "file_search"
            && input
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty()
        {
            return Err(ToolError::InvalidInput("query must not be empty".into()));
        }
        if self.name == "file_write" && input.get("content").and_then(Value::as_str).is_none() {
            return Err(ToolError::InvalidInput("content must be a string".into()));
        }
        Ok(input)
    }

    async fn execute(
        &self,
        input: Value,
        context: ToolContext,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, ToolError> {
        if cancellation.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        let started = Instant::now();
        let scope = self.scope(&context)?;
        let path = scope
            .resolve(Self::input_path(&input)?)
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        let policy = PolicyEngine::from_env();
        policy
            .require(self.policy, true, context.approved)
            .map_err(|_| ToolError::PermissionDenied)?;

        let data = match self.name.as_str() {
            "file_read" => {
                let content = fs::read_to_string(&path)
                    .await
                    .map_err(|error| ToolError::Execution(error.to_string()))?;
                json!({"path": path, "content": content})
            }
            "directory_list" => {
                let mut entries = fs::read_dir(&path)
                    .await
                    .map_err(|error| ToolError::Execution(error.to_string()))?;
                let mut names = Vec::new();
                while let Some(entry) = entries
                    .next_entry()
                    .await
                    .map_err(|error| ToolError::Execution(error.to_string()))?
                {
                    if cancellation.is_cancelled() {
                        return Err(ToolError::Cancelled);
                    }
                    names.push(entry.file_name().to_string_lossy().to_string());
                    if names.len() >= 500 {
                        break;
                    }
                }
                names.sort();
                json!({"path": path, "entries": names})
            }
            "file_search" => {
                let query = input
                    .get("query")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let max_results = input
                    .get("max_results")
                    .and_then(Value::as_u64)
                    .unwrap_or(25)
                    .min(100) as usize;
                let content = fs::read_to_string(&path)
                    .await
                    .map_err(|error| ToolError::Execution(error.to_string()))?;
                let matches: Vec<_> = content
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| line.contains(query))
                    .take(max_results)
                    .map(|(line_number, line)| json!({"line": line_number + 1, "text": line}))
                    .collect();
                json!({"path": path, "query": query, "matches": matches})
            }
            "file_write" => {
                let content = input
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                fs::write(&path, content)
                    .await
                    .map_err(|error| ToolError::Execution(error.to_string()))?;
                json!({"path": path, "bytes": content.len()})
            }
            _ => return Err(ToolError::NotFound(self.name.clone())),
        };
        Ok(ToolResult {
            invocation_id: context.invocation_id,
            tool: self.name.clone(),
            success: true,
            summary: format!("{} completed", self.name),
            data,
            duration_ms: started.elapsed().as_millis() as u64,
            redacted: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyx_core::TaskId;
    use std::fs;
    use uuid::Uuid;

    fn context(root: &std::path::Path, approved: bool) -> ToolContext {
        ToolContext {
            task_id: TaskId::new_v4(),
            invocation_id: Uuid::new_v4(),
            workspace_root: root.to_path_buf(),
            approved,
            target: ExecutionTarget::Host,
        }
    }

    #[tokio::test]
    async fn reads_inside_workspace() {
        let root = std::env::temp_dir().join(format!("nyx-fs-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "Türkçe içerik").unwrap();
        let tool = FileSystemTool::read();
        let input = json!({"path": "note.txt"});
        let ctx = context(&root, false);
        let validated = tool.validate(input, &ctx).await.unwrap();
        let result = tool
            .execute(validated, ctx, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(result.data["content"], "Türkçe içerik");
    }

    #[tokio::test]
    async fn requires_approval_for_write() {
        let root = std::env::temp_dir().join(format!("nyx-fs-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let tool = FileSystemTool::write();
        let input = json!({"path": "note.txt", "content": "hello"});
        let ctx = context(&root, false);
        let validated = tool.validate(input, &ctx).await.unwrap();
        assert!(matches!(
            tool.execute(validated, ctx, CancellationToken::new()).await,
            Err(ToolError::PermissionDenied)
        ));
    }
}
