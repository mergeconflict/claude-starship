//! Wrapper binary that bridges Claude Code status line with Starship prompt.
//!
//! Claude Code passes hook data via stdin, but Starship custom modules can't receive
//! stdin. This wrapper saves the hook data to a temp file, then execs into Starship.
//!
//! Usage: Configure as Claude Code status line command in ~/.claude/settings.json:
//! ```json
//! { "statusLine": { "type": "command", "command": "claude-starship" } }
//! ```

use anyhow::Result;
use claude_starship_common::{HOOK_FILE_PATH, HookData};
use std::fs;
use std::io::{self, Read};

fn main() {
    if run().is_err() {
        // Silent failure - just exec starship without writing hook file
        exec_starship();
    }
}

/// Read hook data from stdin, write to temp file, then exec starship.
fn run() -> Result<()> {
    // Read hook data JSON from stdin
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // Parse and re-serialize to validate JSON
    let hook_data: HookData = serde_json::from_str(&input)?;
    let json = serde_json::to_string(&hook_data)?;

    // Write to temp file for Starship modules to read
    fs::write(HOOK_FILE_PATH, json)?;

    // Exec starship (replaces this process, never returns)
    exec_starship()
}

/// Replace this process with `starship prompt`. Never returns on success.
fn exec_starship() -> ! {
    // Set CLAUDE_STARSHIP=1 so Starship custom modules know we're in a Claude Code session
    // SAFETY: Single-threaded at this point, about to exec anyway
    unsafe { std::env::set_var("CLAUDE_STARSHIP", "1") };
    let err = exec::Command::new("starship").arg("prompt").exec();
    // exec() only returns if it fails
    eprintln!("Failed to exec starship: {err}");
    std::process::exit(1);
}
