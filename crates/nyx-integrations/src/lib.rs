use async_trait::async_trait;
use nyx_core::{ExecutionTarget, PermissionClass};
use nyx_security::{Operation, PolicyEngine};
use nyx_tools::{NyxTool, ToolContext, ToolDescriptor, ToolError, ToolResult};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::time::Instant;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy)]
pub enum IntegrationKind {
    CalendarList,
    CalendarCreate,
    CrmSearch,
    CrmUpsert,
    WordPressMessage,
    ResearchFetch,
}

impl IntegrationKind {
    fn name(self) -> &'static str {
        match self {
            Self::CalendarList => "calendar_list_events",
            Self::CalendarCreate => "calendar_create_event",
            Self::CrmSearch => "crm_search_leads",
            Self::CrmUpsert => "crm_upsert_customer",
            Self::WordPressMessage => "wordpress_send_message",
            Self::ResearchFetch => "research_fetch_source",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::CalendarList => {
                "Read calendar events through the configured calendar REST adapter."
            }
            Self::CalendarCreate => {
                "Create a calendar event through the configured calendar REST adapter."
            }
            Self::CrmSearch => {
                "Search leads and customers through the configured CRM REST adapter."
            }
            Self::CrmUpsert => {
                "Create or update a lead/customer through the configured CRM REST adapter."
            }
            Self::WordPressMessage => {
                "Send a message through the configured WordPress/customer-contact REST adapter."
            }
            Self::ResearchFetch => {
                "Fetch a research source through a configured research gateway for later synthesis."
            }
        }
    }

    fn endpoint_var(self) -> &'static str {
        match self {
            Self::CalendarList | Self::CalendarCreate => "NYX_CALENDAR_URL",
            Self::CrmSearch | Self::CrmUpsert => "NYX_CRM_URL",
            Self::WordPressMessage => "NYX_WORDPRESS_URL",
            Self::ResearchFetch => "NYX_RESEARCH_URL",
        }
    }

    fn token_var(self) -> &'static str {
        match self {
            Self::CalendarList | Self::CalendarCreate => "NYX_CALENDAR_TOKEN",
            Self::CrmSearch | Self::CrmUpsert => "NYX_CRM_TOKEN",
            Self::WordPressMessage => "NYX_WORDPRESS_TOKEN",
            Self::ResearchFetch => "NYX_RESEARCH_TOKEN",
        }
    }

    fn operation(self) -> Operation {
        match self {
            Self::CalendarList | Self::CrmSearch | Self::ResearchFetch => Operation::ExternalRead,
            Self::CalendarCreate | Self::CrmUpsert | Self::WordPressMessage => {
                Operation::ExternalWrite
            }
        }
    }

    fn write(self) -> bool {
        matches!(
            self,
            Self::CalendarCreate | Self::CrmUpsert | Self::WordPressMessage
        )
    }

    fn schema(self) -> Value {
        match self {
            Self::CalendarList => {
                json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"},"query":{"type":"string"}}})
            }
            Self::CalendarCreate => {
                json!({"type":"object","required":["title","start","end"],"properties":{"title":{"type":"string"},"start":{"type":"string"},"end":{"type":"string"},"description":{"type":"string"},"attendees":{"type":"array","items":{"type":"string"}}}})
            }
            Self::CrmSearch => {
                json!({"type":"object","required":["query"],"properties":{"query":{"type":"string"},"limit":{"type":"integer","maximum":100}}})
            }
            Self::CrmUpsert => {
                json!({"type":"object","required":["contact"],"properties":{"contact":{"type":"object"}}})
            }
            Self::WordPressMessage => {
                json!({"type":"object","required":["recipient","message"],"properties":{"recipient":{"type":"string"},"message":{"type":"string","maxLength":10000},"subject":{"type":"string"}}})
            }
            Self::ResearchFetch => {
                json!({"type":"object","required":["query"],"properties":{"query":{"type":"string"},"max_sources":{"type":"integer","maximum":20}}})
            }
        }
    }
}

pub struct IntegrationTool {
    kind: IntegrationKind,
    client: Client,
    policy: PolicyEngine,
}

impl IntegrationTool {
    pub fn new(kind: IntegrationKind) -> Self {
        Self {
            kind,
            client: Client::new(),
            policy: PolicyEngine::from_env(),
        }
    }

    pub fn all() -> Vec<Self> {
        [
            IntegrationKind::CalendarList,
            IntegrationKind::CalendarCreate,
            IntegrationKind::CrmSearch,
            IntegrationKind::CrmUpsert,
            IntegrationKind::WordPressMessage,
            IntegrationKind::ResearchFetch,
        ]
        .into_iter()
        .map(Self::new)
        .collect()
    }

    fn endpoint(&self) -> Result<String, ToolError> {
        std::env::var(self.kind.endpoint_var()).map_err(|_| {
            ToolError::Execution(format!(
                "integration is not configured: {}",
                self.kind.endpoint_var()
            ))
        })
    }

    async fn request(&self, input: &Value) -> Result<Value, ToolError> {
        let url = self.endpoint()?;
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(ToolError::InvalidInput(
                "integration endpoint must be HTTP(S)".into(),
            ));
        }
        let token = std::env::var(self.kind.token_var()).unwrap_or_default();
        let method = if self.kind.write() {
            Method::POST
        } else {
            Method::GET
        };
        let mut request = self.client.request(method.clone(), &url);
        if !token.is_empty() {
            request = request.bearer_auth(token);
        }
        if method == Method::GET {
            request = request.query(&[("payload", input.to_string())]);
        } else {
            request = request.json(input);
        }
        let response = timeout(REQUEST_TIMEOUT, request.send())
            .await
            .map_err(|_| ToolError::Timeout)?
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        if !status.is_success() {
            return Err(ToolError::Execution(format!(
                "{} returned HTTP {}",
                self.kind.name(),
                status
            )));
        }
        serde_json::from_str(&body)
            .or_else(|_| Ok(json!({"raw": body})))
            .map_err(|error: serde_json::Error| ToolError::Execution(error.to_string()))
    }
}

#[async_trait]
impl NyxTool for IntegrationTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: self.kind.name().into(),
            version: "1".into(),
            description: self.kind.description().into(),
            permission: if self.kind.write() {
                PermissionClass::Ask
            } else {
                PermissionClass::Allow
            },
            target: ExecutionTarget::Remote,
            timeout_ms: REQUEST_TIMEOUT.as_millis() as u64,
            idempotent: !self.kind.write(),
            input_schema: self.kind.schema(),
        }
    }

    async fn validate(&self, input: Value, _context: &ToolContext) -> Result<Value, ToolError> {
        if !input.is_object() {
            return Err(ToolError::InvalidInput("input must be an object".into()));
        }
        if matches!(self.kind, IntegrationKind::WordPressMessage)
            && input["message"].as_str().unwrap_or("").trim().is_empty()
        {
            return Err(ToolError::InvalidInput("message must not be empty".into()));
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
        self.policy
            .require(self.kind.operation(), true, context.approved)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let started = Instant::now();
        let data = self.request(&input).await?;
        Ok(ToolResult {
            invocation_id: context.invocation_id,
            tool: self.kind.name().into(),
            success: true,
            summary: format!("{} completed", self.kind.name()),
            data,
            duration_ms: started.elapsed().as_millis() as u64,
            redacted: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyx_core::TaskId;
    use uuid::Uuid;

    #[test]
    fn exposes_service_capability_contracts() {
        let names: Vec<_> = IntegrationTool::all()
            .iter()
            .map(|tool| tool.descriptor().name)
            .collect();
        assert!(names.contains(&"calendar_list_events".to_owned()));
        assert!(names.contains(&"crm_search_leads".to_owned()));
        assert!(names.contains(&"wordpress_send_message".to_owned()));
    }

    #[tokio::test]
    async fn validates_message_input() {
        let tool = IntegrationTool::new(IntegrationKind::WordPressMessage);
        let context = ToolContext {
            task_id: TaskId::new_v4(),
            invocation_id: Uuid::new_v4(),
            workspace_root: std::env::temp_dir(),
            approved: true,
            target: ExecutionTarget::Remote,
        };
        assert!(tool
            .validate(json!({"recipient":"x","message":""}), &context)
            .await
            .is_err());
    }
}
