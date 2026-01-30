//! Shared types for claude-starship workspace.
//!
//! Defines the JSON schema for Claude Code status line hook data.
//! Schema derived from: <https://code.claude.com/docs/en/statusline>

use serde::{Deserialize, Serialize};

/// Temp file path where hook data is written for Starship modules to read.
pub const HOOK_FILE_PATH: &str = "/tmp/claude-starship.json";

/// Root structure for Claude Code status line data passed via stdin.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct HookData {
    /// Name of the hook event (e.g., "Status").
    pub hook_event_name: Option<String>,
    /// Unique identifier for the current Claude Code session.
    pub session_id: String,
    /// Path to the session transcript JSON file.
    pub transcript_path: Option<String>,
    /// Current working directory.
    pub cwd: Option<String>,
    /// Model being used for this session.
    pub model: Model,
    /// Workspace path information.
    pub workspace: Option<Workspace>,
    /// Claude Code version string (e.g., "1.0.80").
    pub version: Option<String>,
    /// Output style configuration.
    pub output_style: Option<OutputStyle>,
    /// Cumulative cost and activity metrics.
    pub cost: Option<Cost>,
    /// Token usage and context window information.
    pub context_window: Option<ContextWindow>,
}

/// Model information.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Model {
    /// Model identifier (e.g., "claude-opus-4-1").
    pub id: String,
    /// Human-readable model name (e.g., "Opus").
    pub display_name: String,
}

/// Workspace paths for the current session.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Workspace {
    /// Current working directory.
    pub current_dir: String,
    /// Original project directory (where Claude Code was started).
    pub project_dir: String,
}

/// Output style configuration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct OutputStyle {
    /// Style name (e.g., "default").
    pub name: String,
}

/// Cumulative cost and activity metrics for the session.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Cost {
    /// Total session cost in USD.
    pub total_cost_usd: f64,
    /// Total session wall-clock duration in milliseconds.
    pub total_duration_ms: u64,
    /// Total time spent waiting for API responses in milliseconds.
    pub total_api_duration_ms: u64,
    /// Total lines of code added during the session.
    pub total_lines_added: u64,
    /// Total lines of code removed during the session.
    pub total_lines_removed: u64,
}

/// Token usage and context window information.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ContextWindow {
    /// Cumulative input tokens across the entire session.
    pub total_input_tokens: u64,
    /// Cumulative output tokens across the entire session.
    pub total_output_tokens: u64,
    /// Maximum context window size for the model.
    pub context_window_size: u64,
    /// Pre-calculated percentage of context window used (0-100).
    pub used_percentage: Option<f64>,
    /// Pre-calculated percentage of context window remaining (0-100).
    pub remaining_percentage: Option<f64>,
    /// Current context window usage from the last API call (null if no messages yet).
    pub current_usage: Option<CurrentUsage>,
}

/// Token usage from the last API call.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CurrentUsage {
    /// Input tokens in current context.
    pub input_tokens: u64,
    /// Output tokens generated.
    pub output_tokens: u64,
    /// Tokens written to cache.
    pub cache_creation_input_tokens: Option<u64>,
    /// Tokens read from cache.
    pub cache_read_input_tokens: Option<u64>,
}
