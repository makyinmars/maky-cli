pub mod exec;
pub mod ls;
pub mod read;
pub mod registry;

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::Value;

use crate::model::types::{ToolCall, ToolResult};

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace_root: PathBuf,
    pub approval_required: bool,
    pub approval_granted: bool,
    pub max_output_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalPolicy {
    AlwaysAsk,
    OnRequest,
    Never,
}

impl ApprovalPolicy {
    pub fn from_config(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "always_ask" => Self::AlwaysAsk,
            "never" => Self::Never,
            _ => Self::OnRequest,
        }
    }
}

pub fn parse_json_args(call: &ToolCall) -> Option<Value> {
    let first_arg = call.args.first()?;
    serde_json::from_str(first_arg).ok().or_else(|| {
        let unescaped = first_arg.replace("\\\"", "\"");
        serde_json::from_str(&unescaped).ok()
    })
}

pub fn truncate_output(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_string(), false);
    }

    let mut out = text.chars().take(max_chars).collect::<String>();
    out.push_str("...<truncated>");
    (out, true)
}

pub fn resolve_workspace_path(
    workspace_root: &std::path::Path,
    requested: &str,
) -> anyhow::Result<PathBuf> {
    let canonical_root = workspace_root
        .canonicalize()
        .map_err(|error| anyhow::anyhow!("failed to canonicalize workspace root: {error}"))?;

    let candidate = if std::path::Path::new(requested).is_absolute() {
        PathBuf::from(requested)
    } else {
        canonical_root.join(requested)
    };

    let canonical_candidate = candidate
        .canonicalize()
        .map_err(|error| anyhow::anyhow!("failed to canonicalize requested path: {error}"))?;

    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(anyhow::anyhow!(
            "path escapes workspace root: {}",
            canonical_candidate.display()
        ));
    }

    Ok(canonical_candidate)
}

#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn name(&self) -> &'static str;

    async fn execute(&self, call: ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
}
