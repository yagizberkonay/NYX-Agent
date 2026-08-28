use nyx_tools::ToolDescriptor;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlannerError {
    #[error("LLM provider is not configured")]
    NotConfigured,
    #[error("LLM request failed: {0}")]
    Request(String),
    #[error("LLM response was invalid: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedToolCall {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerOutput {
    pub summary: String,
    pub calls: Vec<PlannedToolCall>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Clone, Deserialize)]
struct Message {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
struct ToolCall {
    function: FunctionCall,
}

#[derive(Debug, Clone, Deserialize)]
struct FunctionCall {
    name: String,
    arguments: String,
}

#[derive(Clone)]
pub struct Planner {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl Planner {
    pub fn from_env() -> Result<Self, PlannerError> {
        let base_url = std::env::var("OPENAI_API_BASE").map_err(|_| PlannerError::NotConfigured)?;
        let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| PlannerError::NotConfigured)?;
        let model = std::env::var("NYX_PLANNER_MODEL").unwrap_or_else(|_| "gpt-5-mini".into());
        Ok(Self {
            client: Client::new(),
            base_url,
            api_key,
            model,
        })
    }

    pub async fn plan(
        &self,
        request: &str,
        descriptors: &[ToolDescriptor],
    ) -> Result<PlannerOutput, PlannerError> {
        let tools: Vec<Value> = descriptors
            .iter()
            .map(|descriptor| {
                json!({
                    "type":"function",
                    "function": {
                        "name": descriptor.name,
                        "description": descriptor.description,
                        "parameters": descriptor.input_schema,
                    }
                })
            })
            .collect();
        let payload = json!({
            "model": self.model,
            "messages": [
                {"role":"system","content":"You are NYX, a local-first computer agent. Select only tools from the supplied registry. Never invent tool names. Return concise tool calls and a short summary."},
                {"role":"user","content":request}
            ],
            "tools": tools,
            "tool_choice":"auto",
            "max_completion_tokens": 2000,
        });
        let endpoint = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|error| PlannerError::Request(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| PlannerError::Request(error.to_string()))?;
        if !status.is_success() {
            return Err(PlannerError::Request(format!("HTTP {status}")));
        }
        let parsed: ChatResponse = serde_json::from_str(&body)
            .map_err(|error| PlannerError::InvalidResponse(error.to_string()))?;
        let message = parsed
            .choices
            .first()
            .ok_or_else(|| PlannerError::InvalidResponse("no choices".into()))?
            .message
            .clone();
        let allowed: std::collections::HashSet<_> = descriptors
            .iter()
            .map(|descriptor| descriptor.name.as_str())
            .collect();
        let mut calls = Vec::new();
        for call in message.tool_calls.unwrap_or_default() {
            if !allowed.contains(call.function.name.as_str()) {
                return Err(PlannerError::InvalidResponse(format!(
                    "unknown tool: {}",
                    call.function.name
                )));
            }
            let arguments = serde_json::from_str(&call.function.arguments)
                .map_err(|error| PlannerError::InvalidResponse(error.to_string()))?;
            calls.push(PlannedToolCall {
                name: call.function.name,
                arguments,
            });
        }
        Ok(PlannerOutput {
            summary: message.content.unwrap_or_else(|| "Plan hazırlandı".into()),
            calls,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_defaults_to_cost_effective_model() {
        std::env::set_var("OPENAI_API_BASE", "https://example.test/v1");
        std::env::set_var("OPENAI_API_KEY", "test-key");
        std::env::remove_var("NYX_PLANNER_MODEL");
        assert!(Planner::from_env().is_ok());
    }
}
