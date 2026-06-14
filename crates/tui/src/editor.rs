//! `$EDITOR` integration: suspends the TUI, opens the body in the user's
//! editor, then re-enters the alternate screen and returns the edited text.

use std::fs;
use std::io::{self, Stdout, Write};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;

/// Opens `initial` in `$VISUAL`/`$EDITOR`/`vi`, suspending the TUI for the
/// duration. Returns the edited contents, or `None` if the editor failed to
/// launch or exited non-zero. The terminal is always restored to raw +
/// alternate-screen before returning.
pub fn edit_body(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    initial: &str,
) -> io::Result<Option<String>> {
    let path = temp_path();
    fs::write(&path, initial)?;

    // Leave the TUI so the editor owns the terminal.
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let status = Command::new(&editor).arg(&path).status();

    // Always re-enter the TUI, regardless of how the editor fared.
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;

    let result = match status {
        Ok(s) if s.success() => fs::read_to_string(&path).ok(),
        _ => None,
    };
    let _ = fs::remove_file(&path);
    let _ = io::stdout().flush();
    Ok(result)
}

/// A unique temp file path keyed by pid + nanos.
fn temp_path() -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("lazyissue-{pid}-{nanos}.md"))
}
