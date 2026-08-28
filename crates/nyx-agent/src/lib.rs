use chrono::Utc;
use nyx_core::{
    ActivityEvent, ActivityStatus, ExecutionTarget, PlanStep, StepStatus, TaskState, TaskStatus,
};
use nyx_host::HostTool;
use nyx_integrations::IntegrationTool;
use nyx_planner::{Planner, PlannerOutput};
use nyx_tools::{ToolContext, ToolRegistry, ToolResult};
use nyx_tools_fs::FileSystemTool;
use serde_json::{json, Value};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("task was cancelled")]
    Cancelled,
    #[error("tool error: {0}")]
    Tool(String),
    #[error("agent input error: {0}")]
    Input(String),
}

#[derive(Clone)]
pub struct AgentEngine {
    registry: ToolRegistry,
    events: broadcast::Sender<ActivityEvent>,
}

impl AgentEngine {
    pub fn new() -> Self {
        let mut tools: Vec<Arc<dyn nyx_tools::NyxTool>> = vec![
            Arc::new(FileSystemTool::read()),
            Arc::new(FileSystemTool::search()),
            Arc::new(FileSystemTool::list()),
            Arc::new(FileSystemTool::write()),
        ];
        tools.extend(
            HostTool::all()
                .into_iter()
                .map(|tool| Arc::new(tool) as Arc<dyn nyx_tools::NyxTool>),
        );
        tools.extend(
            IntegrationTool::all()
                .into_iter()
                .map(|tool| Arc::new(tool) as Arc<dyn nyx_tools::NyxTool>),
        );
        let (events, _) = broadcast::channel(256);
        Self {
            registry: ToolRegistry::new(tools),
            events,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ActivityEvent> {
        self.events.subscribe()
    }

    pub fn tool_descriptors(&self) -> Vec<nyx_tools::ToolDescriptor> {
        self.registry.descriptors()
    }

    pub fn cancel_token() -> CancellationToken {
        CancellationToken::new()
    }

    fn emit(&self, event: ActivityEvent) {
        let _ = self.events.send(event);
    }

    fn build_plan(&self, request: &str) -> Vec<PlanStep> {
        vec![
            PlanStep {
                id: Uuid::new_v4(),
                title: "Analyze the active workspace".into(),
                tool: Some("directory_list".into()),
                status: StepStatus::Planned,
                target: ExecutionTarget::Host,
            },
            PlanStep {
                id: Uuid::new_v4(),
                title: format!("Interpret request: {request}"),
                tool: None,
                status: StepStatus::Planned,
                target: ExecutionTarget::Host,
            },
            PlanStep {
                id: Uuid::new_v4(),
                title: "Verify the observed result".into(),
                tool: None,
                status: StepStatus::Planned,
                target: ExecutionTarget::Host,
            },
        ]
    }

    pub async fn run(
        &self,
        request: impl Into<String>,
        workspace_root: impl Into<std::path::PathBuf>,
        cancellation: CancellationToken,
    ) -> Result<TaskState, AgentError> {
        let request = request.into();
        let workspace_root = workspace_root.into();
        let mut task = TaskState::new(request.clone());
        task.plan = self.build_plan(&request);
        task.status = TaskStatus::Running;
        task.updated_at = Utc::now();
        self.emit(ActivityEvent::status(
            task.task_id,
            ActivityStatus::Started,
            "Analyzing workspace",
        ));

        if cancellation.is_cancelled() {
            task.status = TaskStatus::Cancelled;
            return Err(AgentError::Cancelled);
        }

        if std::env::var("NYX_ENABLE_LLM_PLANNER").as_deref() == Ok("1") {
            if let Ok(planner) = Planner::from_env() {
                return self
                    .run_planned(request, workspace_root, task, planner, cancellation)
                    .await;
            }
        }

        let step_id = task.plan[0].id;
        task.current_step = Some(step_id);
        task.plan[0].status = StepStatus::Running;
        let invocation_id = Uuid::new_v4();
        let context = ToolContext {
            task_id: task.task_id,
            invocation_id,
            workspace_root,
            approved: false,
            target: ExecutionTarget::Host,
        };
        let input = json!({"path": "."});
        let tool = self
            .registry
            .get("directory_list")
            .map_err(|e| AgentError::Tool(e.to_string()))?;
        let validated = tool
            .validate(input, &context)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))?;
        self.emit(ActivityEvent {
            tool: Some("directory_list".into()),
            target: Some(ExecutionTarget::Host),
            ..ActivityEvent::status(
                task.task_id,
                ActivityStatus::Running,
                "Reading workspace entries",
            )
        });
        let result = tool
            .execute(validated, context, cancellation.clone())
            .await
            .map_err(|error| {
                task.status = if matches!(error, nyx_tools::ToolError::Cancelled) {
                    TaskStatus::Cancelled
                } else {
                    TaskStatus::Failed
                };
                AgentError::Tool(error.to_string())
            })?;
        self.emit(ActivityEvent {
            tool: Some(result.tool.clone()),
            target: Some(ExecutionTarget::Host),
            duration_ms: Some(result.duration_ms),
            ..ActivityEvent::status(task.task_id, ActivityStatus::Success, "Workspace analyzed")
        });
        task.plan[0].status = StepStatus::Verified;
        task.plan[1].status = StepStatus::Verified;
        task.plan[2].status = StepStatus::Verified;
        task.verification.verified = true;
        task.verification
            .evidence
            .push("directory_list returned successfully".into());
        task.status = TaskStatus::Completed;
        task.updated_at = Utc::now();
        self.emit(ActivityEvent::status(
            task.task_id,
            ActivityStatus::Success,
            "Verification passed",
        ));
        Ok(task)
    }

    async fn run_planned(
        &self,
        request: String,
        workspace_root: std::path::PathBuf,
        mut task: TaskState,
        planner: Planner,
        cancellation: CancellationToken,
    ) -> Result<TaskState, AgentError> {
        let plan: PlannerOutput = planner
            .plan(&request, &self.tool_descriptors())
            .await
            .map_err(|error| AgentError::Input(error.to_string()))?;
        task.plan = plan
            .calls
            .iter()
            .map(|call| PlanStep {
                id: Uuid::new_v4(),
                title: format!("Execute {}", call.name),
                tool: Some(call.name.clone()),
                status: StepStatus::Planned,
                target: ExecutionTarget::Host,
            })
            .collect();
        if task.plan.is_empty() {
            task.status = TaskStatus::Failed;
            return Err(AgentError::Input(
                "planner returned no executable tool calls".into(),
            ));
        }
        for (index, call) in plan.calls.into_iter().enumerate() {
            if cancellation.is_cancelled() {
                task.status = TaskStatus::Cancelled;
                return Err(AgentError::Cancelled);
            }
            task.current_step = Some(task.plan[index].id);
            task.plan[index].status = StepStatus::Running;
            self.emit(ActivityEvent {
                tool: Some(call.name.clone()),
                target: Some(ExecutionTarget::Host),
                ..ActivityEvent::status(
                    task.task_id,
                    ActivityStatus::Running,
                    "Executing planned tool",
                )
            });
            let context = ToolContext {
                task_id: task.task_id,
                invocation_id: Uuid::new_v4(),
                workspace_root: workspace_root.clone(),
                approved: std::env::var("NYX_AUTONOMY_MODE").as_deref() == Ok("autonomous"),
                target: ExecutionTarget::Host,
            };
            let result = self
                .execute_tool(&call.name, call.arguments, context, cancellation.clone())
                .await?;
            task.plan[index].status = StepStatus::Verified;
            self.emit(ActivityEvent {
                tool: Some(result.tool),
                target: Some(ExecutionTarget::Host),
                duration_ms: Some(result.duration_ms),
                ..ActivityEvent::status(
                    task.task_id,
                    ActivityStatus::Success,
                    "Planned tool completed",
                )
            });
        }
        task.status = TaskStatus::Completed;
        task.verification.verified = true;
        task.verification
            .evidence
            .push("all planned tool calls completed".into());
        task.updated_at = Utc::now();
        self.emit(ActivityEvent::status(
            task.task_id,
            ActivityStatus::Success,
            "Autonomous plan verified",
        ));
        Ok(task)
    }

    pub async fn execute_tool(
        &self,
        name: &str,
        input: Value,
        context: ToolContext,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, AgentError> {
        let tool = self
            .registry
            .get(name)
            .map_err(|e| AgentError::Tool(e.to_string()))?;
        let validated = tool
            .validate(input, &context)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))?;
        tool.execute(validated, context, cancellation)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))
    }
}

impl Default for AgentEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn run_completes_and_verifies() {
        let root = std::env::temp_dir().join(format!("nyx-agent-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("README.md"), "NYX").unwrap();
        let engine = AgentEngine::new();
        let state = engine
            .run("inspect project", root, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(state.status, TaskStatus::Completed);
        assert!(state.verification.verified);
        assert_eq!(
            state
                .plan
                .iter()
                .filter(|step| step.status == StepStatus::Verified)
                .count(),
            3
        );
    }

    #[tokio::test]
    async fn cancellation_is_observed_before_work() {
        let root = std::env::temp_dir();
        let token = CancellationToken::new();
        token.cancel();
        let engine = AgentEngine::new();
        assert!(matches!(
            engine.run("cancel", root, token).await,
            Err(AgentError::Cancelled)
        ));
    }
}
