# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

claude-starship integrates Claude Code session metrics into the Starship shell prompt. It solves the problem that Starship custom modules can't receive stdin, but Claude Code passes hook data via stdin.

**Architecture**:
```
Claude Code → claude-starship (reads stdin, writes to temp file) → exec starship prompt
                                                                      ↓
                                          Starship custom modules ← claude-metric (reads temp file)
```

## Build Commands

```bash
cargo build --release                        # Build all crates
cargo nextest run                            # Run all tests
cargo nextest run -p claude-metric test_name # Run single test
cargo clippy --all-targets                   # Lint (pedantic enabled at workspace level)
cargo install --path claude-starship         # Install wrapper binary
cargo install --path claude-metric           # Install metric binary
```

## Workspace Structure

- **claude-starship**: Wrapper binary - reads hook JSON from stdin, writes to `/tmp/claude-starship.json`, sets `CLAUDE_STARSHIP=1` env var, then execs `starship prompt`
- **claude-metric**: Metric reader binary - subcommands (`cost`, `tokens`, `model`, `context`, `response`, etc.) output formatted metrics for Starship modules
- **claude-starship-common**: Shared types (`HookData`, `Model`, `Cost`, `ContextWindow`, etc.)

## Code Standards

This project follows:
- [Microsoft Rust Guidelines](https://microsoft.github.io/rust-guidelines/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

Clippy pedantic lints are enabled at the workspace level. Any `#[allow]` attributes must include a justifying comment.

## Design Decisions

- **Silent failures**: Output nothing if temp file missing or field null - never print errors to avoid breaking prompts
- **Fixed temp file path**: `HOOK_FILE_PATH` constant in `claude-starship-common` (currently `/tmp/claude-starship.json`)
- **Process replacement**: Use `exec()` syscall so claude-starship replaces itself with starship (no child process overhead)
- **Colored output**: Response time percentiles use ANSI codes (green p50, yellow p90, red p99)

## Hook Data Schema

No official JSON schema exists. Types in `claude-starship-common` are derived from Claude Code docs. The `HookData` struct has many optional fields - handle missing data gracefully.
