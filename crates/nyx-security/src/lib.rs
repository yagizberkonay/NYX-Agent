use nyx_core::PermissionClass;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SecurityError {
    #[error("path is outside the workspace scope")]
    OutsideWorkspace,
    #[error("path cannot be resolved")]
    InvalidPath,
    #[error("operation is denied by policy")]
    Denied,
    #[error("command is not allowed by policy")]
    CommandDenied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceScope {
    root: PathBuf,
}

impl WorkspaceScope {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, SecurityError> {
        let root = std::fs::canonicalize(root).map_err(|_| SecurityError::InvalidPath)?;
        if !root.is_dir() {
            return Err(SecurityError::InvalidPath);
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, candidate: impl AsRef<Path>) -> Result<PathBuf, SecurityError> {
        let candidate = candidate.as_ref();
        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.root.join(candidate)
        };

        let canonical = if joined.exists() {
            std::fs::canonicalize(&joined).map_err(|_| SecurityError::InvalidPath)?
        } else {
            let parent = joined.parent().ok_or(SecurityError::InvalidPath)?;
            let canonical_parent =
                std::fs::canonicalize(parent).map_err(|_| SecurityError::InvalidPath)?;
            canonical_parent.join(joined.file_name().ok_or(SecurityError::InvalidPath)?)
        };

        if canonical == self.root || canonical.starts_with(&self.root) {
            Ok(canonical)
        } else {
            Err(SecurityError::OutsideWorkspace)
        }
    }

    pub fn contains(&self, candidate: impl AsRef<Path>) -> bool {
        self.resolve(candidate).is_ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    FileRead,
    FileSearch,
    FileWrite,
    FileDelete,
    ShellReadOnly,
    ShellExecute,
    GitDiff,
    GitCommit,
    GitPush,
    ProductionDeploy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone)]
pub struct PolicyEngine {
    allow_shell_read_only: bool,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self {
            allow_shell_read_only: true,
        }
    }
}

impl PolicyEngine {
    pub fn decide(&self, operation: Operation, in_workspace: bool) -> PolicyDecision {
        if !in_workspace {
            return PolicyDecision::Deny;
        }
        match operation {
            Operation::FileRead | Operation::FileSearch | Operation::GitDiff => {
                PolicyDecision::Allow
            }
            Operation::ShellReadOnly if self.allow_shell_read_only => PolicyDecision::Allow,
            Operation::FileWrite
            | Operation::FileDelete
            | Operation::ShellExecute
            | Operation::GitCommit
            | Operation::GitPush
            | Operation::ProductionDeploy => PolicyDecision::Ask,
            Operation::ShellReadOnly => PolicyDecision::Ask,
        }
    }

    pub fn require(
        &self,
        operation: Operation,
        in_workspace: bool,
        approved: bool,
    ) -> Result<(), SecurityError> {
        match self.decide(operation, in_workspace) {
            PolicyDecision::Allow => Ok(()),
            PolicyDecision::Ask if approved => Ok(()),
            PolicyDecision::Ask => Err(SecurityError::Denied),
            PolicyDecision::Deny => Err(SecurityError::Denied),
        }
    }
}

pub fn redact_secrets(input: &str, secrets: &[&str]) -> String {
    secrets.iter().fold(input.to_owned(), |value, secret| {
        if secret.is_empty() {
            value
        } else {
            value.replace(secret, "[REDACTED]")
        }
    })
}

pub fn validate_shell_command(command: &str) -> Result<(), SecurityError> {
    let normalized = command.to_ascii_lowercase();
    let blocked = [
        "rm -rf /",
        "format c:",
        "shutdown",
        "reboot",
        "del /s /q",
        "drop database",
        "curl | sh",
        "wget | sh",
    ];
    if blocked.iter().any(|pattern| normalized.contains(pattern)) {
        return Err(SecurityError::CommandDenied);
    }
    Ok(())
}

pub fn permission_class(decision: PolicyDecision) -> PermissionClass {
    match decision {
        PolicyDecision::Allow => PermissionClass::Allow,
        PolicyDecision::Ask => PermissionClass::Ask,
        PolicyDecision::Deny => PermissionClass::Deny,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    fn temp_scope() -> WorkspaceScope {
        let path = std::env::temp_dir().join(format!("nyx-security-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("temp workspace");
        WorkspaceScope::new(&path).expect("valid scope")
    }

    #[test]
    fn rejects_parent_escape() {
        let scope = temp_scope();
        assert_eq!(
            scope.resolve("../outside"),
            Err(SecurityError::OutsideWorkspace)
        );
    }

    #[test]
    fn allows_new_file_inside_scope() {
        let scope = temp_scope();
        let resolved = scope.resolve("nested.txt").expect("new file path is safe");
        assert!(resolved.starts_with(scope.root()));
    }

    #[test]
    fn policy_requires_approval_for_writes() {
        let policy = PolicyEngine::default();
        assert_eq!(
            policy.decide(Operation::FileWrite, true),
            PolicyDecision::Ask
        );
        assert!(policy.require(Operation::FileWrite, true, false).is_err());
        assert!(policy.require(Operation::FileWrite, true, true).is_ok());
    }

    #[test]
    fn policy_denies_outside_scope() {
        let policy = PolicyEngine::default();
        assert_eq!(
            policy.decide(Operation::FileRead, false),
            PolicyDecision::Deny
        );
    }

    #[test]
    fn redacts_all_secrets() {
        let output = redact_secrets("token=abc and key=xyz", &["abc", "xyz"]);
        assert_eq!(output, "token=[REDACTED] and key=[REDACTED]");
    }

    #[test]
    fn blocks_obvious_dangerous_shell_patterns() {
        assert!(validate_shell_command("rm -rf /").is_err());
        assert!(validate_shell_command("cargo test").is_ok());
    }
}
