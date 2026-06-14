//! `lazyissue` — a lazygit-style terminal UI for the local-first issue tool.
//!
//! The entry point sets up the terminal (raw mode + alternate screen), installs
//! a panic hook that restores it before printing, runs the event loop, and
//! always restores the terminal on exit via a Drop guard.

mod app;
mod editor;
mod event;
mod form;
mod ui;

use std::io::{self, Stdout};
use std::process::ExitCode;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;

use app::{App, Panel};
use event::{AppEvent, EventLoop};
use form::Field;

/// Restores the terminal to its normal state when dropped, so even an early
/// return or `?` leaves the user's terminal usable.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(TerminalGuard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn main() -> ExitCode {
    install_panic_hook();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lazyissue: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Restores the terminal before the default panic message is printed, so a
/// panic doesn't leave the terminal in raw/alt-screen mode.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default(info);
    }));
}

fn run() -> io::Result<()> {
    let dir = issue_core::storage::resolve_issue_dir();
    let mut app = App::new(dir.clone())?;

    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let events = EventLoop::new(&dir);

    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, &app))?;

        match events.next() {
            Some(AppEvent::Key(key)) => handle_key(&mut app, &mut terminal, key)?,
            Some(AppEvent::FilesChanged) => app.reload(),
            None => break,
        }
    }
    Ok(())
}

/// Dispatches a key press to the right handler based on current mode
/// (modal > search-input > normal).
fn handle_key(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    key: KeyEvent,
) -> io::Result<()> {
    if app.modal.is_some() {
        handle_modal_key(app, key);
        return Ok(());
    }
    if app.searching {
        handle_search_key(app, key);
        return Ok(());
    }
    handle_normal_key(app, terminal, key)
}

fn handle_modal_key(app: &mut App, key: KeyEvent) {
    let Some(form) = app.modal.as_mut() else { return };
    match key.code {
        KeyCode::Esc => app.cancel_modal(),
        KeyCode::Enter => app.submit_modal(),
        KeyCode::Tab => form.focus_next(),
        KeyCode::BackTab => form.focus_prev(),
        KeyCode::Backspace => form.backspace(),
        KeyCode::Char(' ') if form.focus == Field::Status => form.toggle_status(),
        KeyCode::Char(c) => form.input_char(c),
        _ => {}
    }
}

fn handle_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.clear_search(),
        KeyCode::Enter => app.confirm_search(),
        KeyCode::Backspace => app.search_backspace(),
        KeyCode::Char(c) => app.search_push(c),
        _ => {}
    }
}

fn handle_normal_key(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    key: KeyEvent,
) -> io::Result<()> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        // Esc precedence: modal (handled earlier) -> search -> help -> quit.
        KeyCode::Esc => {
            if app.search.is_some() {
                app.clear_search();
            } else if app.show_help {
                app.show_help = false;
            } else {
                app.should_quit = true;
            }
        }
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.show_help = !app.show_help,

        // Movement.
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('g') => app.move_first(),
        KeyCode::Char('G') => app.move_last(),
        KeyCode::Char('d') if ctrl => app.half_page_down(),
        KeyCode::Char('u') if ctrl => app.half_page_up(),

        // Focus.
        KeyCode::Tab => app.focus_next(),
        KeyCode::BackTab => app.focus_prev(),
        KeyCode::Char('h') | KeyCode::Left => app.focus_prev(),
        KeyCode::Char('l') | KeyCode::Right => app.focus_next(),

        // Filters pane: Enter/Space apply the highlighted filter row.
        KeyCode::Enter | KeyCode::Char(' ') if app.focus == Panel::Filters => {
            app.apply_selected_filter();
        }

        // Mutations / forms.
        KeyCode::Char('c') => app.close_selected(),
        KeyCode::Char('o') => app.reopen_selected(),
        KeyCode::Char('n') => app.open_create_form(),
        KeyCode::Char('e') => app.open_edit_form(),
        KeyCode::Char('b') => edit_selected_body(app, terminal)?,

        // Search / reload.
        KeyCode::Char('/') => app.start_search(),
        KeyCode::Char('R') => {
            app.reload();
            app.status_line = "reloaded".to_string();
        }
        _ => {}
    }
    Ok(())
}

/// Suspends the TUI, edits the selected issue's body in `$EDITOR`, and applies
/// the result.
fn edit_selected_body(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> io::Result<()> {
    if app.selected_issue().is_none() {
        app.status_line = "no issue selected".to_string();
        return Ok(());
    }
    let initial = app.selected_body();
    match editor::edit_body(terminal, &initial)? {
        Some(body) => app.apply_body_edit(body),
        None => app.status_line = "edit cancelled".to_string(),
    }
    Ok(())
}
