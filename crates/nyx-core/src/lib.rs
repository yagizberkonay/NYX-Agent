use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type TaskId = Uuid;
pub type StepId = Uuid;
pub type InvocationId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Planned,
    Running,
    AwaitingPermission,
    Completed,
    Failed,
    Cancelled,
    NeedsReview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Planned,
    AwaitingPermission,
    Running,
    Observed,
    Retrying,
    Verified,
    Failed,
    Cancelled,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTarget {
    Host,
    Sandbox,
    Container,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionClass {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: StepId,
    pub title: String,
    pub tool: Option<String>,
    pub status: StepStatus,
    pub target: ExecutionTarget,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerificationState {
    pub verified: bool,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub task_id: TaskId,
    pub goal: String,
    pub status: TaskStatus,
    pub plan: Vec<PlanStep>,
    pub current_step: Option<StepId>,
    pub verification: VerificationState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TaskState {
    pub fn new(goal: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            task_id: Uuid::new_v4(),
            goal: goal.into(),
            status: TaskStatus::Planned,
            plan: Vec::new(),
            current_step: None,
            verification: VerificationState::default(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus {
    Started,
    Running,
    Success,
    Error,
    WaitingForPermission,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub event_id: Uuid,
    pub task_id: TaskId,
    pub timestamp: DateTime<Utc>,
    pub status: ActivityStatus,
    pub message: String,
    pub tool: Option<String>,
    pub target: Option<ExecutionTarget>,
    pub duration_ms: Option<u64>,
    pub error_code: Option<String>,
}

impl ActivityEvent {
    pub fn status(task_id: TaskId, status: ActivityStatus, message: impl Into<String>) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            task_id,
            timestamp: Utc::now(),
            status,
            message: message.into(),
            tool: None,
            target: None,
            duration_ms: None,
            error_code: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_task_starts_planned_and_unverified() {
        let task = TaskState::new("Inspect a project");
        assert_eq!(task.status, TaskStatus::Planned);
        assert!(!task.verification.verified);
        assert_eq!(task.goal, "Inspect a project");
    }

    #[test]
    fn activity_event_serializes_stably() {
        let task_id = Uuid::new_v4();
        let event = ActivityEvent::status(task_id, ActivityStatus::Success, "Verification passed");
        let value = serde_json::to_value(event).expect("event is serializable");
        assert_eq!(value["status"], "success");
        assert_eq!(value["message"], "Verification passed");
    }
}
