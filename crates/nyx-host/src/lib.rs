use async_trait::async_trait;
use nyx_core::{ExecutionTarget, PermissionClass};
use nyx_security::{validate_shell_command, Operation, PolicyEngine, WorkspaceScope};
use nyx_tools::{NyxTool, ToolContext, ToolDescriptor, ToolError, ToolResult};
use serde_json::{json, Value};
use std::time::Instant;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

const HOST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy)]
pub enum HostToolKind {
    OpenApp,
    OpenUrl,
    ProcessList,
    ShellExec,
    MediaControl,
    Notify,
}

impl HostToolKind {
    fn name(self) -> &'static str {
        match self {
            Self::OpenApp => "host_open_app",
            Self::OpenUrl => "host_open_url",
            Self::ProcessList => "host_process_list",
            Self::ShellExec => "host_shell_exec",
            Self::MediaControl => "host_media_control",
            Self::Notify => "host_notify",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::OpenApp => "Open an allowlisted desktop application or service.",
            Self::OpenUrl => "Open a validated HTTP(S) URL in the default browser.",
            Self::ProcessList => "Inspect currently running processes without modifying them.",
            Self::ShellExec => "Execute a bounded shell command inside the active workspace.",
            Self::MediaControl => "Control the active local media player.",
            Self::Notify => "Send a local desktop notification.",
        }
    }

    fn permission(self) -> PermissionClass {
        match self {
            Self::ProcessList | Self::OpenUrl | Self::Notify | Self::MediaControl => {
                PermissionClass::Allow
            }
            Self::OpenApp | Self::ShellExec => PermissionClass::Ask,
        }
    }

    fn schema(self) -> Value {
        match self {
            Self::OpenApp => json!({
                "type":"object", "required":["app"],
                "properties":{"app":{"type":"string"},"args":{"type":"array","items":{"type":"string"}}}
            }),
            Self::OpenUrl => {
                json!({"type":"object","required":["url"],"properties":{"url":{"type":"string","pattern":"^https?://"}}})
            }
            Self::ProcessList => json!({"type":"object","properties":{"filter":{"type":"string"}}}),
            Self::ShellExec => {
                json!({"type":"object","required":["command"],"properties":{"command":{"type":"string","minLength":1},"cwd":{"type":"string"}}})
            }
            Self::MediaControl => {
                json!({"type":"object","required":["action"],"properties":{"action":{"enum":["play","pause","play_pause","next","previous","stop"]}}})
            }
            Self::Notify => {
                json!({"type":"object","required":["title","message"],"properties":{"title":{"type":"string"},"message":{"type":"string"}}})
            }
        }
    }
}

pub struct HostTool {
    kind: HostToolKind,
    policy: PolicyEngine,
}

impl HostTool {
    pub fn new(kind: HostToolKind) -> Self {
        Self {
            kind,
            policy: PolicyEngine::from_env(),
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::new(HostToolKind::OpenApp),
            Self::new(HostToolKind::OpenUrl),
            Self::new(HostToolKind::ProcessList),
            Self::new(HostToolKind::ShellExec),
            Self::new(HostToolKind::MediaControl),
            Self::new(HostToolKind::Notify),
        ]
    }

    async fn run_program(program: &str, args: &[&str]) -> Result<String, ToolError> {
        let output = timeout(HOST_TIMEOUT, Command::new(program).args(args).output())
            .await
            .map_err(|_| ToolError::Timeout)?
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        if !output.status.success() {
            return Err(ToolError::Execution(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    async fn open_url(url: &str) -> Result<(), ToolError> {
        if !(url.starts_with("https://") || url.starts_with("http://")) || url.contains('\n') {
            return Err(ToolError::InvalidInput(
                "only HTTP(S) URLs are allowed".into(),
            ));
        }
        #[cfg(target_os = "windows")]
        {
            Self::run_program("cmd", &["/C", "start", "", url]).await?;
        }
        #[cfg(target_os = "macos")]
        {
            Self::run_program("open", &[url]).await?;
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            Self::run_program("xdg-open", &[url]).await?;
        }
        Ok(())
    }

    async fn open_app(app: &str, args: &[String]) -> Result<(), ToolError> {
        let app = app.trim().to_ascii_lowercase();
        let url = match app.as_str() {
            "youtube" => Some("https://www.youtube.com"),
            "spotify" => Some("https://open.spotify.com"),
            _ => None,
        };
        if let Some(url) = url {
            return Self::open_url(url).await;
        }
        let allowed = [
            "calculator",
            "terminal",
            "files",
            "browser",
            "vscode",
            "code",
            "notepad",
        ];
        if !allowed.contains(&app.as_str()) {
            return Err(ToolError::InvalidInput(format!(
                "application is not allowlisted: {app}"
            )));
        }
        let program = match app.as_str() {
            "calculator" => {
                if cfg!(target_os = "windows") {
                    "calc"
                } else if cfg!(target_os = "macos") {
                    "open"
                } else {
                    "gnome-calculator"
                }
            }
            "terminal" => {
                if cfg!(target_os = "windows") {
                    "cmd"
                } else if cfg!(target_os = "macos") {
                    "open"
                } else {
                    "x-terminal-emulator"
                }
            }
            "files" => {
                if cfg!(target_os = "windows") {
                    "explorer"
                } else if cfg!(target_os = "macos") {
                    "open"
                } else {
                    "xdg-open"
                }
            }
            "browser" => {
                if cfg!(target_os = "windows") {
                    "cmd"
                } else if cfg!(target_os = "macos") {
                    "open"
                } else {
                    "xdg-open"
                }
            }
            "vscode" | "code" => "code",
            "notepad" => {
                if cfg!(target_os = "windows") {
                    "notepad"
                } else {
                    "gedit"
                }
            }
            _ => unreachable!(),
        };
        let mut command = Command::new(program);
        #[cfg(target_os = "windows")]
        if app == "terminal" {
            command.args(["/C"]);
        }
        #[cfg(target_os = "macos")]
        if matches!(
            app.as_str(),
            "calculator" | "terminal" | "files" | "browser"
        ) {
            command.arg("-a").arg(app);
        }
        #[cfg(not(target_os = "macos"))]
        if matches!(app.as_str(), "browser" | "files") {
            command.arg(".");
        }
        command.args(args);
        command
            .spawn()
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        Ok(())
    }

    fn result(
        context: &ToolContext,
        name: &str,
        started: Instant,
        summary: String,
        data: Value,
    ) -> ToolResult {
        ToolResult {
            invocation_id: context.invocation_id,
            tool: name.into(),
            success: true,
            summary,
            data,
            duration_ms: started.elapsed().as_millis() as u64,
            redacted: false,
        }
    }
}

#[async_trait]
impl NyxTool for HostTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: self.kind.name().into(),
            version: "1".into(),
            description: self.kind.description().into(),
            permission: self.kind.permission(),
            target: ExecutionTarget::Host,
            timeout_ms: HOST_TIMEOUT.as_millis() as u64,
            idempotent: !matches!(self.kind, HostToolKind::ShellExec),
            input_schema: self.kind.schema(),
        }
    }

    async fn validate(&self, input: Value, context: &ToolContext) -> Result<Value, ToolError> {
        if !input.is_object() {
            return Err(ToolError::InvalidInput("input must be an object".into()));
        }
        match self.kind {
            HostToolKind::OpenUrl => {
                let url = input
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::InvalidInput("url is required".into()))?;
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    return Err(ToolError::InvalidInput(
                        "only HTTP(S) URLs are allowed".into(),
                    ));
                }
            }
            HostToolKind::OpenApp => {
                if input
                    .get("app")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err(ToolError::InvalidInput("app is required".into()));
                }
            }
            HostToolKind::ShellExec => {
                let command = input
                    .get("command")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::InvalidInput("command is required".into()))?;
                validate_shell_command(command)
                    .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
                let cwd = input.get("cwd").and_then(Value::as_str).unwrap_or(".");
                WorkspaceScope::new(&context.workspace_root)
                    .map_err(|error| ToolError::InvalidInput(error.to_string()))?
                    .resolve(cwd)
                    .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
            }
            HostToolKind::MediaControl => {
                if !["play", "pause", "play_pause", "next", "previous", "stop"]
                    .contains(&input.get("action").and_then(Value::as_str).unwrap_or(""))
                {
                    return Err(ToolError::InvalidInput("unsupported media action".into()));
                }
            }
            HostToolKind::Notify => {
                if input
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .is_empty()
                    || input
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .is_empty()
                {
                    return Err(ToolError::InvalidInput(
                        "title and message are required".into(),
                    ));
                }
            }
            HostToolKind::ProcessList => {}
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
        let name = self.kind.name();
        let in_workspace = true;
        let operation = match self.kind {
            HostToolKind::OpenApp => Operation::ShellExecute,
            HostToolKind::OpenUrl => Operation::ShellReadOnly,
            HostToolKind::ProcessList => Operation::ShellReadOnly,
            HostToolKind::ShellExec => Operation::ShellExecute,
            HostToolKind::MediaControl => Operation::ShellExecute,
            HostToolKind::Notify => Operation::ShellReadOnly,
        };
        self.policy
            .require(operation, in_workspace, context.approved)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        match self.kind {
            HostToolKind::OpenUrl => {
                let url = input["url"].as_str().unwrap();
                Self::open_url(url).await?;
                Ok(Self::result(
                    &context,
                    name,
                    started,
                    "Opened URL".into(),
                    json!({"url":url}),
                ))
            }
            HostToolKind::OpenApp => {
                let app = input["app"].as_str().unwrap();
                let args = input
                    .get("args")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Self::open_app(app, &args).await?;
                Ok(Self::result(
                    &context,
                    name,
                    started,
                    format!("Opened application: {app}"),
                    json!({"app":app}),
                ))
            }
            HostToolKind::ProcessList => {
                #[cfg(target_os = "windows")]
                let output = Self::run_program("tasklist", &[]).await?;
                #[cfg(not(target_os = "windows"))]
                let output = Self::run_program("ps", &["-eo", "pid,comm,%cpu,%mem"]).await?;
                let filter = input
                    .get("filter")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let lines: Vec<_> = output
                    .lines()
                    .filter(|line| filter.is_empty() || line.to_ascii_lowercase().contains(&filter))
                    .take(200)
                    .collect();
                Ok(Self::result(
                    &context,
                    name,
                    started,
                    "Listed running processes".into(),
                    json!({"processes":lines}),
                ))
            }
            HostToolKind::ShellExec => {
                let command = input["command"].as_str().unwrap();
                let cwd = input.get("cwd").and_then(Value::as_str).unwrap_or(".");
                let root = WorkspaceScope::new(&context.workspace_root)
                    .map_err(|error| ToolError::Execution(error.to_string()))?;
                let cwd = root
                    .resolve(cwd)
                    .map_err(|error| ToolError::Execution(error.to_string()))?;
                #[cfg(target_os = "windows")]
                let output = Command::new("cmd")
                    .args(["/C", command])
                    .current_dir(cwd)
                    .output();
                #[cfg(not(target_os = "windows"))]
                let output = Command::new("sh")
                    .args(["-lc", command])
                    .current_dir(cwd)
                    .output();
                let output = timeout(HOST_TIMEOUT, output)
                    .await
                    .map_err(|_| ToolError::Timeout)?
                    .map_err(|error| ToolError::Execution(error.to_string()))?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Ok(Self::result(
                    &context,
                    name,
                    started,
                    "Shell command completed".into(),
                    json!({"exit_code":output.status.code(),"stdout":stdout,"stderr":stderr}),
                ))
            }
            HostToolKind::MediaControl => {
                let action = input["action"].as_str().unwrap();
                #[cfg(target_os = "windows")]
                let _ = action;
                #[cfg(target_os = "macos")]
                {
                    let script = format!("tell application \"Spotify\" to {action}");
                    Self::run_program("osascript", &["-e", &script]).await?;
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    Self::run_program("playerctl", &[action]).await?;
                }
                Ok(Self::result(
                    &context,
                    name,
                    started,
                    format!("Media action: {action}"),
                    json!({"action":action}),
                ))
            }
            HostToolKind::Notify => {
                let title = input["title"].as_str().unwrap();
                let message = input["message"].as_str().unwrap();
                #[cfg(target_os = "windows")]
                let _ = (title, message);
                #[cfg(target_os = "macos")]
                {
                    let script =
                        format!("display notification {:?} with title {:?}", message, title);
                    Self::run_program("osascript", &["-e", &script]).await?;
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    Self::run_program("notify-send", &[title, message]).await?;
                }
                Ok(Self::result(
                    &context,
                    name,
                    started,
                    "Desktop notification sent".into(),
                    json!({"title":title}),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyx_core::TaskId;
    use uuid::Uuid;

    #[test]
    fn exposes_host_control_tools() {
        let names: Vec<_> = HostTool::all()
            .iter()
            .map(|tool| tool.descriptor().name)
            .collect();
        assert!(names.contains(&"host_open_app".to_owned()));
        assert!(names.contains(&"host_shell_exec".to_owned()));
    }

    #[tokio::test]
    async fn rejects_untrusted_url() {
        let tool = HostTool::new(HostToolKind::OpenUrl);
        let context = ToolContext {
            task_id: TaskId::new_v4(),
            invocation_id: Uuid::new_v4(),
            workspace_root: std::env::temp_dir(),
            approved: true,
            target: ExecutionTarget::Host,
        };
        assert!(tool
            .validate(json!({"url":"file:///etc/passwd"}), &context)
            .await
            .is_err());
    }
}
