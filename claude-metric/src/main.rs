//! Metric reader for Starship custom modules.
//!
//! Reads Claude Code hook data from the temp file and outputs a single metric.
//! Designed to be called by Starship custom modules.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use clap::{Parser, ValueEnum};
use claude_starship_common::{HOOK_FILE_PATH, HookData};
use jiff::SignedDuration;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};

#[derive(Parser)]
#[command(name = "claude-metric")]
#[command(about = "Display Claude Code session metrics for Starship prompts")]
struct Cli {
    /// The metric to display
    #[arg(value_enum)]
    metric: Metric,
}

#[derive(Clone, ValueEnum)]
enum Metric {
    /// Official session cost from Claude Code in USD (e.g., "$0.02")
    Cost,
    /// Total tokens in thousands (e.g., "55k")
    Tokens,
    /// Input tokens only in thousands (e.g., "50k")
    Input,
    /// Output tokens only in thousands (e.g., "5k")
    Output,
    /// Lines added/removed (e.g., "+150 -30")
    Lines,
    /// Model display name (e.g., "Opus")
    Model,
    /// Context window usage percentage (e.g., "28%")
    Context,
    /// Session wall-clock duration (e.g., "45s")
    Duration,
    /// Time spent waiting for API responses (e.g., "12s")
    Api,
    /// Claude Code version (e.g., "1.0.80")
    Version,
    /// Number of messages in conversation (e.g., "12")
    Messages,
    /// Average response time in seconds (e.g., "3.2s")
    Response,
    /// Estimated cost in 5-hour billing window, calculated from transcript (e.g., "$1.23")
    Block,
    /// Estimated cost today, calculated from transcript (e.g., "$5.67")
    Today,
}

/// Transcript entry with timestamp for time-based filtering.
#[derive(Deserialize)]
struct TranscriptEntry {
    #[serde(rename = "type")]
    entry_type: String,
    timestamp: Option<DateTime<Utc>>,
    message: Option<AssistantMessage>,
}

/// Assistant message containing usage and model data.
#[derive(Deserialize)]
struct AssistantMessage {
    model: Option<String>,
    usage: Option<TokenUsage>,
}

/// Token usage from a single API call.
#[derive(Deserialize)]
#[allow(clippy::struct_field_names)] // Field names match Claude API JSON schema
struct TokenUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

/// Pricing per million tokens for a Claude model.
/// These are our own estimates based on published pricing, not from an official API.
/// Source: <https://platform.claude.com/docs/en/about-claude/pricing>
#[derive(Clone, Copy)]
struct ModelPricing {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
}

impl ModelPricing {
    const OPUS_4_5: Self = Self {
        input: 5.0,
        output: 25.0,
        cache_read: 0.50,
        cache_write: 6.25,
    };
    const OPUS_4: Self = Self {
        input: 15.0,
        output: 75.0,
        cache_read: 1.50,
        cache_write: 18.75,
    };
    const SONNET_4: Self = Self {
        input: 3.0,
        output: 15.0,
        cache_read: 0.30,
        cache_write: 3.75,
    };
    const HAIKU_4_5: Self = Self {
        input: 1.0,
        output: 5.0,
        cache_read: 0.10,
        cache_write: 1.25,
    };
    const HAIKU_3_5: Self = Self {
        input: 0.80,
        output: 4.0,
        cache_read: 0.08,
        cache_write: 1.00,
    };
    const SONNET_3_5: Self = Self {
        input: 3.0,
        output: 15.0,
        cache_read: 0.30,
        cache_write: 3.75,
    };
    const HAIKU_3: Self = Self {
        input: 0.25,
        output: 1.25,
        cache_read: 0.03,
        cache_write: 0.30,
    };

    /// Look up pricing by model ID. Falls back to Sonnet 4 pricing for unknown models.
    fn for_model(model_id: &str) -> Self {
        if model_id.contains("opus-4-5") || model_id.contains("opus-4.5") {
            Self::OPUS_4_5
        } else if model_id.contains("opus-4") {
            Self::OPUS_4
        } else if model_id.contains("sonnet-3-5") || model_id.contains("sonnet-3.5") {
            Self::SONNET_3_5
        } else if model_id.contains("haiku-4-5") || model_id.contains("haiku-4.5") {
            Self::HAIKU_4_5
        } else if model_id.contains("haiku-3-5") || model_id.contains("haiku-3.5") {
            Self::HAIKU_3_5
        } else if model_id.contains("haiku-3") {
            Self::HAIKU_3
        } else {
            // Default to Sonnet 4 pricing (most common)
            Self::SONNET_4
        }
    }
}

impl TokenUsage {
    /// Calculate cost in USD using the given model pricing.
    #[allow(clippy::cast_precision_loss)] // Token counts fit comfortably in f64 mantissa
    fn cost(&self, pricing: ModelPricing) -> f64 {
        let input = self.input_tokens.unwrap_or(0) as f64;
        let output = self.output_tokens.unwrap_or(0) as f64;
        let cache_read = self.cache_read_input_tokens.unwrap_or(0) as f64;
        let cache_write = self.cache_creation_input_tokens.unwrap_or(0) as f64;

        (input * pricing.input
            + output * pricing.output
            + cache_read * pricing.cache_read
            + cache_write * pricing.cache_write)
            / 1_000_000.0
    }
}

#[allow(clippy::cast_precision_loss)] // Token counts fit in f64 mantissa
#[allow(clippy::cast_possible_wrap)] // Duration ms won't exceed i64::MAX
fn main() -> Result<()> {
    let cli = Cli::parse();

    let contents = fs::read_to_string(HOOK_FILE_PATH)
        .with_context(|| format!("Failed to read {HOOK_FILE_PATH}"))?;

    let hook_data: HookData =
        serde_json::from_str(&contents).context("Failed to parse hook data")?;

    match cli.metric {
        Metric::Cost => {
            if let Some(cost) = &hook_data.cost {
                println!("${:.2}", cost.total_cost_usd);
            }
        }
        Metric::Tokens => {
            if let Some(ctx) = &hook_data.context_window {
                let total = ctx.total_input_tokens + ctx.total_output_tokens;
                println!("{}k", total / 1000);
            }
        }
        Metric::Input => {
            if let Some(ctx) = &hook_data.context_window {
                println!("{}k", ctx.total_input_tokens / 1000);
            }
        }
        Metric::Output => {
            if let Some(ctx) = &hook_data.context_window {
                println!("{}k", ctx.total_output_tokens / 1000);
            }
        }
        Metric::Lines => {
            if let Some(cost) = &hook_data.cost {
                println!("+{} -{}", cost.total_lines_added, cost.total_lines_removed);
            }
        }
        Metric::Model => {
            println!("{}", hook_data.model.display_name);
        }
        Metric::Context => {
            if let Some(ctx) = &hook_data.context_window {
                if let Some(pct) = ctx.used_percentage {
                    println!("{pct:.0}%");
                } else if ctx.context_window_size > 0 {
                    let used = ctx.total_input_tokens + ctx.total_output_tokens;
                    let pct = (used as f64 / ctx.context_window_size as f64) * 100.0;
                    println!("{pct:.0}%");
                }
            }
        }
        Metric::Duration => {
            if let Some(cost) = &hook_data.cost {
                let duration = SignedDuration::from_millis(cost.total_duration_ms as i64);
                let truncated = SignedDuration::from_mins(duration.as_mins());
                println!("{truncated:#}");
            }
        }
        Metric::Api => {
            if let Some(cost) = &hook_data.cost {
                let secs = cost.total_api_duration_ms / 1000;
                println!("{secs}s");
            }
        }
        Metric::Version => {
            if let Some(version) = &hook_data.version {
                println!("{version}");
            }
        }
        Metric::Messages => {
            if let Some(path) = &hook_data.transcript_path
                && let Ok(count) = count_messages(path)
            {
                println!("{count}");
            }
        }
        Metric::Response => {
            if let Some(path) = &hook_data.transcript_path
                && let Ok(output) = response_time_percentiles(path)
                && !output.is_empty()
            {
                println!("{output}");
            }
        }
        Metric::Block => {
            if let Some(path) = &hook_data.transcript_path {
                let cutoff = Utc::now() - Duration::hours(5);
                if let Ok(cost) = sum_cost_since(path, cutoff) {
                    println!("${cost:.2}");
                }
            }
        }
        Metric::Today => {
            if let Some(path) = &hook_data.transcript_path {
                let today = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
                let cutoff = DateTime::<Utc>::from_naive_utc_and_offset(today, Utc);
                if let Ok(cost) = sum_cost_since(path, cutoff) {
                    println!("${cost:.2}");
                }
            }
        }
    }

    Ok(())
}

/// Count user and assistant messages in the transcript.
fn count_messages(path: &str) -> Result<u64> {
    let file = File::open(path).context("Failed to open transcript")?;
    let reader = BufReader::new(file);

    let count = reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<TranscriptEntry>(&line).ok())
        .filter(|entry| entry.entry_type == "user" || entry.entry_type == "assistant")
        .count() as u64;

    Ok(count)
}

/// Calculate response time percentiles (user message to assistant response).
/// Returns p50, plus p90 if ≥10 responses, plus p99 if ≥100 responses.
#[allow(clippy::cast_precision_loss)] // Intentional: milliseconds fit in f64 mantissa
fn response_time_percentiles(path: &str) -> Result<String> {
    // ANSI color codes: green for p50, yellow for p90, red for p99
    const GREEN: &str = "\x1b[32m";
    const YELLOW: &str = "\x1b[33m";
    const RED: &str = "\x1b[31m";
    const RESET: &str = "\x1b[0m";

    let file = File::open(path).context("Failed to open transcript")?;
    let reader = BufReader::new(file);

    // Pair each user message with the next assistant response
    let mut times: Vec<_> = reader
        .lines()
        .map_while(|line| {
            line.ok()
                .and_then(|l| serde_json::from_str::<TranscriptEntry>(&l).ok())
        })
        .scan(None::<DateTime<Utc>>, |last_user_time, entry| {
            Some(match entry.entry_type.as_str() {
                "user" => {
                    *last_user_time = entry.timestamp;
                    None
                }
                "assistant" => last_user_time
                    .take()
                    .zip(entry.timestamp)
                    .map(|(u, a)| (a - u).num_milliseconds()),
                _ => None,
            })
        })
        .flatten()
        .collect();

    if times.is_empty() {
        return Ok(String::new());
    }

    times.sort_unstable();
    let n = times.len();

    let percentile = |p: usize| -> f64 {
        let idx = (n * p / 100).saturating_sub(1).min(n - 1);
        times[idx] as f64 / 1000.0
    };

    let p50 = percentile(50);
    Ok(if n >= 100 {
        format!(
            "{GREEN}{:.1}s{RESET}/{YELLOW}{:.1}s{RESET}/{RED}{:.1}s{RESET}",
            p50,
            percentile(90),
            percentile(99)
        )
    } else if n >= 10 {
        format!(
            "{GREEN}{:.1}s{RESET}/{YELLOW}{:.1}s{RESET}",
            p50,
            percentile(90)
        )
    } else {
        format!("{GREEN}{p50:.1}s{RESET}")
    })
}

/// Estimate cost of all assistant messages since a given cutoff time.
/// Uses our own pricing table, not official costs from Claude Code.
fn sum_cost_since(path: &str, cutoff: DateTime<Utc>) -> Result<f64> {
    let file = File::open(path).context("Failed to open transcript")?;
    let reader = BufReader::new(file);

    let total_cost = reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<TranscriptEntry>(&line).ok())
        .filter(|entry| entry.entry_type == "assistant")
        .filter(|entry| entry.timestamp.is_some_and(|ts| ts >= cutoff))
        .filter_map(|entry| {
            let msg = entry.message?;
            let usage = msg.usage?;
            let pricing = ModelPricing::for_model(msg.model.as_deref().unwrap_or(""));
            Some(usage.cost(pricing))
        })
        .sum();

    Ok(total_cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_model_pricing_for_model_opus_4_5() {
        let pricing = ModelPricing::for_model("claude-opus-4-5-20251101");
        assert!((pricing.input - 5.0).abs() < 0.001);
        assert!((pricing.output - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_model_pricing_for_model_sonnet_4() {
        let pricing = ModelPricing::for_model("claude-sonnet-4-20250514");
        assert!((pricing.input - 3.0).abs() < 0.001);
        assert!((pricing.output - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_model_pricing_for_model_haiku() {
        let pricing = ModelPricing::for_model("claude-haiku-4-5-20250101");
        assert!((pricing.input - 1.0).abs() < 0.001);
        assert!((pricing.output - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_model_pricing_for_model_sonnet_3_5() {
        let pricing = ModelPricing::for_model("claude-sonnet-3-5-20241022");
        assert!((pricing.input - 3.0).abs() < 0.001);
        assert!((pricing.output - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_model_pricing_for_model_unknown_defaults_to_sonnet() {
        let pricing = ModelPricing::for_model("unknown-model");
        assert!((pricing.input - 3.0).abs() < 0.001);
        assert!((pricing.output - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_token_usage_cost_sonnet() {
        let usage = TokenUsage {
            input_tokens: Some(1_000_000),
            output_tokens: Some(1_000_000),
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        };
        // 1M input @ $3 + 1M output @ $15 = $18
        assert!((usage.cost(ModelPricing::SONNET_4) - 18.0).abs() < 0.001);
    }

    #[test]
    fn test_token_usage_cost_opus_4_5() {
        let usage = TokenUsage {
            input_tokens: Some(1_000_000),
            output_tokens: Some(1_000_000),
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        };
        // 1M input @ $5 + 1M output @ $25 = $30
        assert!((usage.cost(ModelPricing::OPUS_4_5) - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_token_usage_cost_with_cache() {
        let usage = TokenUsage {
            input_tokens: Some(1_000_000),
            output_tokens: Some(1_000_000),
            cache_creation_input_tokens: Some(1_000_000),
            cache_read_input_tokens: Some(1_000_000),
        };
        // Sonnet: 1M @ $3 + 1M @ $15 + 1M @ $3.75 + 1M @ $0.30 = $22.05
        assert!((usage.cost(ModelPricing::SONNET_4) - 22.05).abs() < 0.001);
    }

    #[test]
    fn test_token_usage_cost_empty() {
        let usage = TokenUsage {
            input_tokens: None,
            output_tokens: None,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        };
        assert!((usage.cost(ModelPricing::SONNET_4) - 0.0).abs() < 0.001);
    }

    fn create_transcript_file(entries: &[&str]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        for entry in entries {
            writeln!(file, "{entry}").unwrap();
        }
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_count_messages_basic() {
        let file = create_transcript_file(&[
            r#"{"type": "user", "timestamp": "2026-01-30T10:00:00Z"}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:05Z"}"#,
            r#"{"type": "user", "timestamp": "2026-01-30T10:00:10Z"}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:15Z"}"#,
        ]);
        let count = count_messages(file.path().to_str().unwrap()).unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn test_count_messages_filters_other_types() {
        let file = create_transcript_file(&[
            r#"{"type": "user", "timestamp": "2026-01-30T10:00:00Z"}"#,
            r#"{"type": "progress", "timestamp": "2026-01-30T10:00:02Z"}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:05Z"}"#,
            r#"{"type": "file-history-snapshot"}"#,
        ]);
        let count = count_messages(file.path().to_str().unwrap()).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_count_messages_empty_file() {
        let file = create_transcript_file(&[]);
        let count = count_messages(file.path().to_str().unwrap()).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_response_time_percentiles_basic() {
        let file = create_transcript_file(&[
            r#"{"type": "user", "timestamp": "2026-01-30T10:00:00Z"}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:05Z"}"#,
            r#"{"type": "user", "timestamp": "2026-01-30T10:00:10Z"}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:13Z"}"#,
        ]);
        // Two responses: 5s and 3s, p50 = 3s (sorted: [3, 5], median is first half)
        let output = response_time_percentiles(file.path().to_str().unwrap()).unwrap();
        assert_eq!(output, "\x1b[32m3.0s\x1b[0m");
    }

    #[test]
    fn test_response_time_percentiles_no_pairs() {
        let file =
            create_transcript_file(&[r#"{"type": "user", "timestamp": "2026-01-30T10:00:00Z"}"#]);
        let result = response_time_percentiles(file.path().to_str().unwrap()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_response_time_percentiles_ignores_orphaned_assistant() {
        let file = create_transcript_file(&[
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:00Z"}"#,
            r#"{"type": "user", "timestamp": "2026-01-30T10:00:05Z"}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:10Z"}"#,
        ]);
        // Only the second pair counts: 5 seconds
        let output = response_time_percentiles(file.path().to_str().unwrap()).unwrap();
        assert_eq!(output, "\x1b[32m5.0s\x1b[0m");
    }

    #[test]
    fn test_response_time_percentiles_shows_p90_with_10_responses() {
        // Generate 10 user/assistant pairs with varying response times
        let mut entries = Vec::new();
        for i in 0..10 {
            let user_time = format!("2026-01-30T10:{i:02}:00Z");
            let assistant_time = format!("2026-01-30T10:{:02}:{:02}Z", i, (i + 1) % 60);
            entries.push(format!(
                r#"{{"type": "user", "timestamp": "{user_time}"}}"#
            ));
            entries.push(format!(
                r#"{{"type": "assistant", "timestamp": "{assistant_time}"}}"#
            ));
        }
        let entries_ref: Vec<&str> = entries.iter().map(std::string::String::as_str).collect();
        let file = create_transcript_file(&entries_ref);
        let output = response_time_percentiles(file.path().to_str().unwrap()).unwrap();
        // Should show p50/p90 format with colors (green/yellow)
        assert!(
            output.contains('/'),
            "Expected p50/p90 format, got: {output}"
        );
        assert!(
            output.contains("\x1b[32m") && output.contains("\x1b[33m"),
            "Expected green and yellow ANSI codes"
        );
    }

    #[test]
    fn test_sum_cost_since_with_sonnet_model() {
        let file = create_transcript_file(&[
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:00Z", "message": {"model": "claude-sonnet-4-20250514", "usage": {"input_tokens": 1000, "output_tokens": 100}}}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:05Z", "message": {"model": "claude-sonnet-4-20250514", "usage": {"input_tokens": 2000, "output_tokens": 200}}}"#,
        ]);
        let cutoff = "2026-01-30T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cost = sum_cost_since(file.path().to_str().unwrap(), cutoff).unwrap();
        // Sonnet: (1000 + 2000) * 3 / 1M + (100 + 200) * 15 / 1M = 0.009 + 0.0045 = 0.0135
        assert!((cost - 0.0135).abs() < 0.0001);
    }

    #[test]
    fn test_sum_cost_since_with_opus_model() {
        let file = create_transcript_file(&[
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:00Z", "message": {"model": "claude-opus-4-5-20251101", "usage": {"input_tokens": 1000000, "output_tokens": 0}}}"#,
        ]);
        let cutoff = "2026-01-30T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cost = sum_cost_since(file.path().to_str().unwrap(), cutoff).unwrap();
        // Opus 4.5: 1M * $5 / 1M = $5
        assert!((cost - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_sum_cost_since_filters_by_cutoff() {
        let file = create_transcript_file(&[
            r#"{"type": "assistant", "timestamp": "2026-01-30T08:00:00Z", "message": {"model": "claude-sonnet-4", "usage": {"input_tokens": 1000000, "output_tokens": 0}}}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T12:00:00Z", "message": {"model": "claude-sonnet-4", "usage": {"input_tokens": 1000000, "output_tokens": 0}}}"#,
        ]);
        let cutoff = "2026-01-30T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cost = sum_cost_since(file.path().to_str().unwrap(), cutoff).unwrap();
        // Only second entry counts: 1M * $3 / 1M = $3
        assert!((cost - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_sum_cost_since_ignores_non_assistant() {
        let file = create_transcript_file(&[
            r#"{"type": "user", "timestamp": "2026-01-30T10:00:00Z"}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:05Z", "message": {"model": "claude-sonnet-4", "usage": {"input_tokens": 1000000, "output_tokens": 0}}}"#,
        ]);
        let cutoff = "2026-01-30T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cost = sum_cost_since(file.path().to_str().unwrap(), cutoff).unwrap();
        assert!((cost - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_sum_cost_since_handles_missing_usage() {
        let file = create_transcript_file(&[
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:00Z", "message": {}}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:05Z", "message": {"model": "claude-sonnet-4", "usage": {"input_tokens": 1000000, "output_tokens": 0}}}"#,
        ]);
        let cutoff = "2026-01-30T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cost = sum_cost_since(file.path().to_str().unwrap(), cutoff).unwrap();
        assert!((cost - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_sum_cost_since_mixed_models() {
        let file = create_transcript_file(&[
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:00Z", "message": {"model": "claude-sonnet-4", "usage": {"input_tokens": 1000000, "output_tokens": 0}}}"#,
            r#"{"type": "assistant", "timestamp": "2026-01-30T10:00:05Z", "message": {"model": "claude-opus-4-5", "usage": {"input_tokens": 1000000, "output_tokens": 0}}}"#,
        ]);
        let cutoff = "2026-01-30T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cost = sum_cost_since(file.path().to_str().unwrap(), cutoff).unwrap();
        // Sonnet: 1M * $3 = $3, Opus 4.5: 1M * $5 = $5, Total: $8
        assert!((cost - 8.0).abs() < 0.001);
    }
}
