use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("connector is not allowed: {0}")]
    NotAllowed(String),
    #[error("connector runtime is unavailable: {0}")]
    Runtime(String),
    #[error("connector returned invalid data: {0}")]
    InvalidData(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectorDescriptor {
    pub server: String,
    pub display_name: String,
    pub enabled: bool,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorInvocation {
    pub server: String,
    pub tool: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorResult {
    pub server: String,
    pub tool: String,
    pub output: Value,
}

#[derive(Debug, Clone)]
pub struct ConnectorRegistry {
    allowed: HashSet<String>,
    catalog: Vec<ConnectorDescriptor>,
}

impl Default for ConnectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectorRegistry {
    pub fn new() -> Self {
        let catalog = vec![
            (
                "github",
                "GitHub",
                "Repositories, issues, pull requests, and code collaboration.",
            ),
            (
                "vercel",
                "Vercel",
                "Projects, deployments, logs, domains, and production web delivery.",
            ),
            (
                "supabase",
                "Supabase",
                "Projects, Postgres, migrations, Edge Functions, logs, and advisors.",
            ),
            (
                "gmail",
                "Gmail",
                "Inbox search, thread reading, labels, drafts, and mail operations.",
            ),
            (
                "google-calendar",
                "Google Calendar",
                "Schedule discovery and calendar event management.",
            ),
            (
                "google-workspace",
                "Google Workspace",
                "Drive, Docs, Sheets, and Workspace content operations.",
            ),
            (
                "airtable",
                "Airtable",
                "Structured records and lightweight CRM/database operations.",
            ),
            ("apollo", "Apollo", "Lead and prospect research workflows."),
            (
                "asana",
                "Asana",
                "Project, task, and team workflow operations.",
            ),
            (
                "exa",
                "Exa",
                "Web and research retrieval for evidence-backed analysis.",
            ),
            (
                "firecrawl",
                "Firecrawl",
                "Web crawling and page extraction for research.",
            ),
            (
                "hubspot",
                "HubSpot",
                "CRM contacts, companies, deals, and follow-up workflows.",
            ),
            ("hunter", "Hunter", "Domain and contact research."),
            (
                "notion",
                "Notion",
                "Workspace pages, databases, and knowledge management.",
            ),
            (
                "slack",
                "Slack",
                "Team channel search and messaging workflows.",
            ),
            (
                "trello",
                "Trello",
                "Boards, lists, cards, and task follow-up.",
            ),
            (
                "wordpress",
                "WordPress",
                "Site content and publishing workflows.",
            ),
        ]
        .into_iter()
        .map(|(server, display_name, purpose)| ConnectorDescriptor {
            server: server.into(),
            display_name: display_name.into(),
            enabled: true,
            purpose: purpose.into(),
        })
        .collect::<Vec<_>>();
        let allowed = catalog.iter().map(|item| item.server.clone()).collect();
        Self { allowed, catalog }
    }

    pub fn descriptors(&self) -> Vec<ConnectorDescriptor> {
        self.catalog.clone()
    }

    pub async fn invoke(
        &self,
        request: ConnectorInvocation,
    ) -> Result<ConnectorResult, ConnectorError> {
        if !self.allowed.contains(&request.server) {
            return Err(ConnectorError::NotAllowed(request.server));
        }
        let payload = serde_json::to_string(&serde_json::json!({
            "server": request.server,
            "tool": request.tool,
            "input": request.input,
        }))
        .map_err(|error| ConnectorError::InvalidData(error.to_string()))?;
        let mut child = Command::new("manus-mcp-cli")
            .args([
                "tool",
                "call",
                &request.tool,
                "--server",
                &request.server,
                "--input",
                &payload,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ConnectorError::Runtime("missing connector stdout".into()))?;
        let mut lines = BufReader::new(stdout).lines();
        let mut output = String::new();
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?
        {
            output.push_str(&line);
            output.push('\n');
        }
        let status = child
            .wait()
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        if !status.success() {
            return Err(ConnectorError::Runtime(output));
        }
        Ok(ConnectorResult {
            server: request.server,
            tool: request.tool,
            output: Value::String(output),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_core_connectors() {
        let registry = ConnectorRegistry::new();
        let names: HashSet<_> = registry
            .descriptors()
            .into_iter()
            .map(|item| item.server)
            .collect();
        assert!(names.contains("github"));
        assert!(names.contains("gmail"));
        assert!(names.contains("supabase"));
        assert!(names.contains("vercel"));
    }
}
