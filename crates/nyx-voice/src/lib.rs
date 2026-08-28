use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("sidecar process failed to start: {0}")]
    Start(#[from] std::io::Error),
    #[error("sidecar returned invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("sidecar timed out")]
    Timeout,
    #[error("voice operation cancelled")]
    Cancelled,
    #[error("sidecar returned an error: {0}")]
    Provider(String),
    #[error("sidecar closed stdout without a response")]
    EmptyResponse,
}

#[derive(Debug, Clone)]
pub struct SidecarConfig {
    pub program: String,
    pub script: PathBuf,
    pub working_directory: Option<PathBuf>,
    pub timeout: Duration,
}

impl SidecarConfig {
    pub fn python(script: impl Into<PathBuf>) -> Self {
        Self {
            program: "python3".into(),
            script: script.into(),
            working_directory: None,
            timeout: Duration::from_secs(180),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    pub ok: bool,
    pub provider: String,
    pub model: Option<String>,
    pub language: Option<String>,
    pub text: Option<String>,
    pub segments: Option<Vec<TranscriptSegment>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisResult {
    pub ok: bool,
    pub provider: String,
    pub output_path: Option<String>,
    pub sample_rate: Option<u32>,
    pub experimental: Option<bool>,
    pub error: Option<String>,
}

async fn kill_child(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

async fn request(
    config: &SidecarConfig,
    payload: Value,
    cancellation: CancellationToken,
) -> Result<Value, VoiceError> {
    let mut command = Command::new(&config.program);
    command
        .arg(&config.script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(directory) = &config.working_directory {
        command.current_dir(directory);
    }
    let mut child = command.spawn()?;
    let mut stdin = child.stdin.take().ok_or(VoiceError::EmptyResponse)?;
    let stdout = child.stdout.take().ok_or(VoiceError::EmptyResponse)?;
    let line = serde_json::to_string(&payload)? + "\n";
    stdin.write_all(line.as_bytes()).await?;
    stdin.shutdown().await?;
    let mut reader = BufReader::new(stdout).lines();

    let response = tokio::select! {
        _ = cancellation.cancelled() => {
            kill_child(&mut child).await;
            return Err(VoiceError::Cancelled);
        }
        result = timeout(config.timeout, reader.next_line()) => {
            match result {
                Ok(Ok(Some(line))) => line,
                Ok(Ok(None)) => {
                    kill_child(&mut child).await;
                    return Err(VoiceError::EmptyResponse);
                }
                Ok(Err(error)) => {
                    kill_child(&mut child).await;
                    return Err(VoiceError::Start(error));
                }
                Err(_) => {
                    kill_child(&mut child).await;
                    return Err(VoiceError::Timeout);
                }
            }
        }
    };
    let _ = timeout(Duration::from_secs(2), child.wait()).await;
    let parsed: Value = serde_json::from_str(&response)?;
    if parsed.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(VoiceError::Provider(
            parsed
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error")
                .to_owned(),
        ));
    }
    Ok(parsed)
}

#[derive(Debug, Clone)]
pub struct WhisperProvider {
    config: SidecarConfig,
    model: String,
    language: String,
}

impl WhisperProvider {
    pub fn new(config: SidecarConfig) -> Self {
        Self {
            config,
            model: "base".into(),
            language: "tr".into(),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    pub async fn health(&self, cancellation: CancellationToken) -> Result<Value, VoiceError> {
        request(
            &self.config,
            serde_json::json!({"action": "health"}),
            cancellation,
        )
        .await
    }

    pub async fn transcribe(
        &self,
        audio_path: impl Into<String>,
        cancellation: CancellationToken,
    ) -> Result<Transcription, VoiceError> {
        let response = request(
            &self.config,
            serde_json::json!({
                "action": "transcribe",
                "audio_path": audio_path.into(),
                "model": self.model,
                "language": self.language,
                "task": "transcribe"
            }),
            cancellation,
        )
        .await?;
        Ok(serde_json::from_value(response)?)
    }
}

#[derive(Debug, Clone)]
pub struct QwenTurkishProvider {
    config: SidecarConfig,
    base_model: String,
    adapter_dir: String,
    device: String,
}

impl QwenTurkishProvider {
    pub fn new(
        config: SidecarConfig,
        base_model: impl Into<String>,
        adapter_dir: impl Into<String>,
    ) -> Self {
        Self {
            config,
            base_model: base_model.into(),
            adapter_dir: adapter_dir.into(),
            device: "auto".into(),
        }
    }

    pub fn with_device(mut self, device: impl Into<String>) -> Self {
        self.device = device.into();
        self
    }

    pub async fn health(&self, cancellation: CancellationToken) -> Result<Value, VoiceError> {
        request(
            &self.config,
            serde_json::json!({"action": "health"}),
            cancellation,
        )
        .await
    }

    pub async fn synthesize(
        &self,
        text: impl Into<String>,
        output_path: impl Into<String>,
        cancellation: CancellationToken,
    ) -> Result<SynthesisResult, VoiceError> {
        let response = request(
            &self.config,
            serde_json::json!({
                "action": "synthesize",
                "text": text.into(),
                "output_path": output_path.into(),
                "base_model": self.base_model,
                "adapter_dir": self.adapter_dir,
                "device": self.device,
                "language": "Turkish"
            }),
            cancellation,
        )
        .await?;
        Ok(serde_json::from_value(response)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_turkish_and_configurable() {
        let provider = WhisperProvider::new(SidecarConfig::python("server.py"));
        assert_eq!(provider.language, "tr");
        assert_eq!(provider.model, "base");
    }

    #[test]
    fn transcription_schema_is_stable() {
        let value = serde_json::json!({
            "ok": true,
            "provider": "whisper",
            "model": "base",
            "language": "tr",
            "text": "Merhaba",
            "segments": [],
            "error": null
        });
        let parsed: Transcription = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.text.as_deref(), Some("Merhaba"));
    }
}
