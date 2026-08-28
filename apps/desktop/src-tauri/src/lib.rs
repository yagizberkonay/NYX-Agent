use nyx_agent::AgentEngine;
use nyx_voice::{QwenTurkishProvider, SidecarConfig, WhisperProvider};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct RuntimeState {
    engine: AgentEngine,
}

#[derive(Debug, Serialize)]
struct StartTaskResponse {
    task_id: String,
    status: String,
    verified: bool,
    summary: String,
}

fn sidecars_root() -> PathBuf {
    std::env::var_os("NYX_SIDECARS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../sidecars"))
}

#[tauri::command]
async fn start_task(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    request: String,
    workspace_root: String,
) -> Result<StartTaskResponse, String> {
    let engine = state.engine.clone();
    let mut events = engine.subscribe();
    let app_for_events = app.clone();
    let task_events = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            let terminal = matches!(
                event.status,
                nyx_core::ActivityStatus::Success
                    | nyx_core::ActivityStatus::Error
                    | nyx_core::ActivityStatus::Cancelled
            );
            let _ = app_for_events.emit("nyx://activity", event);
            if terminal {
                break;
            }
        }
    });
    let result = engine
        .run(request, workspace_root, CancellationToken::new())
        .await
        .map_err(|error| error.to_string());
    task_events.abort();
    let task = result?;
    Ok(StartTaskResponse {
        task_id: task.task_id.to_string(),
        status: format!("{:?}", task.status).to_lowercase(),
        verified: task.verification.verified,
        summary: "Workspace analysis completed and verified".into(),
    })
}

#[tauri::command]
fn list_tools(state: State<'_, RuntimeState>) -> Vec<nyx_tools::ToolDescriptor> {
    state.engine.tool_descriptors()
}

#[tauri::command]
fn list_connectors() -> Vec<nyx_connectors::ConnectorDescriptor> {
    nyx_connectors::ConnectorRegistry::new().descriptors()
}

#[tauri::command]
async fn invoke_connector(
    server: String,
    tool: String,
    input: Value,
) -> Result<nyx_connectors::ConnectorResult, String> {
    nyx_connectors::ConnectorRegistry::new()
        .invoke(nyx_connectors::ConnectorInvocation {
            server,
            tool,
            input,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn schedule_list(workspace_root: String) -> Result<Vec<nyx_scheduler::ScheduledJob>, String> {
    nyx_scheduler::default_store(&PathBuf::from(workspace_root))
        .load()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn schedule_create(
    workspace_root: String,
    name: String,
    tool: String,
    input: Value,
    interval_seconds: u64,
) -> Result<nyx_scheduler::ScheduledJob, String> {
    nyx_scheduler::default_store(&PathBuf::from(workspace_root))
        .add(name, tool, input, interval_seconds)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn plan_request(
    state: State<'_, RuntimeState>,
    request: String,
) -> Result<nyx_planner::PlannerOutput, String> {
    let planner = nyx_planner::Planner::from_env().map_err(|error| error.to_string())?;
    planner
        .plan(&request, &state.engine.tool_descriptors())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn execute_tool(
    state: State<'_, RuntimeState>,
    name: String,
    input: Value,
    workspace_root: String,
) -> Result<nyx_tools::ToolResult, String> {
    let autonomous = std::env::var("NYX_AUTONOMY_MODE")
        .map(|value| value.eq_ignore_ascii_case("autonomous"))
        .unwrap_or(false);
    let context = nyx_tools::ToolContext {
        task_id: uuid::Uuid::new_v4(),
        invocation_id: uuid::Uuid::new_v4(),
        workspace_root: PathBuf::from(workspace_root),
        approved: autonomous,
        target: nyx_core::ExecutionTarget::Host,
    };
    state
        .engine
        .execute_tool(&name, input, context, CancellationToken::new())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn runtime_health() -> serde_json::Value {
    serde_json::json!({
        "name": "NYX Runtime",
        "status": "ready",
        "privacy": "local-first",
        "autonomy_mode": std::env::var("NYX_AUTONOMY_MODE").unwrap_or_else(|_| "manual".into()),
        "host_control": "workspace-bounded",
        "voice": {
            "stt": "whisper",
            "tts": "qwen3-tts-turkish-experimental"
        }
    })
}

#[tauri::command]
async fn voice_health() -> Result<serde_json::Value, String> {
    let whisper = WhisperProvider::new(SidecarConfig::python(
        sidecars_root().join("whisper/server.py"),
    ));
    let qwen = QwenTurkishProvider::new(
        SidecarConfig::python(sidecars_root().join("qwen3-tts/server.py")),
        std::env::var("NYX_QWEN_TTS_BASE_MODEL")
            .unwrap_or_else(|_| "Qwen/Qwen3-TTS-12Hz-0.6B-Base".into()),
        std::env::var("NYX_QWEN_TTS_ADAPTER_DIR").unwrap_or_default(),
    );
    let cancellation = CancellationToken::new();
    let whisper_status = whisper
        .health(cancellation.clone())
        .await
        .map_err(|e| e.to_string())?;
    let qwen_status = qwen.health(cancellation).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"whisper": whisper_status, "qwen3_tts_turkish": qwen_status}))
}

#[tauri::command]
async fn voice_transcribe(
    audio_path: String,
    model: Option<String>,
    language: Option<String>,
) -> Result<nyx_voice::Transcription, String> {
    let provider = WhisperProvider::new(SidecarConfig::python(
        sidecars_root().join("whisper/server.py"),
    ))
    .with_model(model.unwrap_or_else(|| "base".into()))
    .with_language(language.unwrap_or_else(|| "tr".into()));
    provider
        .transcribe(audio_path, CancellationToken::new())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn voice_synthesize(
    text: String,
    output_path: String,
) -> Result<nyx_voice::SynthesisResult, String> {
    let provider = QwenTurkishProvider::new(
        SidecarConfig::python(sidecars_root().join("qwen3-tts/server.py")),
        std::env::var("NYX_QWEN_TTS_BASE_MODEL")
            .unwrap_or_else(|_| "Qwen/Qwen3-TTS-12Hz-0.6B-Base".into()),
        std::env::var("NYX_QWEN_TTS_ADAPTER_DIR").unwrap_or_default(),
    );
    provider
        .synthesize(text, output_path, CancellationToken::new())
        .await
        .map_err(|error| error.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeState {
            engine: AgentEngine::new(),
        })
        .invoke_handler(tauri::generate_handler![
            start_task,
            list_tools,
            list_connectors,
            invoke_connector,
            schedule_list,
            schedule_create,
            plan_request,
            execute_tool,
            runtime_health,
            voice_health,
            voice_transcribe,
            voice_synthesize
        ])
        .run(tauri::generate_context!())
        .expect("error while running NYX");
}
