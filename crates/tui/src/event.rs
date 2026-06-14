//! Event multiplexing. Two sources feed a single `mpsc` channel of
//! [`AppEvent`]: a thread blocking on terminal key reads, and a `notify`
//! watcher on the issue directory whose raw events are debounced into a single
//! [`AppEvent::FilesChanged`] after a quiet period.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};

/// Quiet period after which a burst of fs events collapses into one event.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// Events delivered to the main loop.
pub enum AppEvent {
    Key(KeyEvent),
    FilesChanged,
}

/// Owns the event channel and keeps the fs watcher alive for the program's
/// lifetime (dropping it stops watching).
pub struct EventLoop {
    rx: Receiver<AppEvent>,
    // Held so the watcher thread is not dropped.
    _watcher: Option<RecommendedWatcher>,
}

impl EventLoop {
    /// Spawns the key-reader thread and (best-effort) the fs watcher.
    pub fn new(watch_dir: &Path) -> Self {
        let (tx, rx) = mpsc::channel::<AppEvent>();

        spawn_key_reader(tx.clone());
        let watcher = spawn_fs_watcher(watch_dir, tx);

        EventLoop {
            rx,
            _watcher: watcher,
        }
    }

    /// Blocks for the next event. Returns `None` only if all senders have
    /// dropped (which should not happen while the app runs).
    pub fn next(&self) -> Option<AppEvent> {
        self.rx.recv().ok()
    }
}

/// Reads terminal events in a dedicated thread, forwarding key presses.
fn spawn_key_reader(tx: Sender<AppEvent>) {
    thread::spawn(move || loop {
        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                if tx.send(AppEvent::Key(key)).is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    });
}

/// Sets up a `notify` watcher on `watch_dir`; raw events are funneled into a
/// debounce thread that emits a single [`AppEvent::FilesChanged`] per quiet
/// burst. Returns `None` if the watcher could not be created (e.g. dir missing
/// at startup) — the TUI still works, only without live reload.
fn spawn_fs_watcher(watch_dir: &Path, tx: Sender<AppEvent>) -> Option<RecommendedWatcher> {
    let (raw_tx, raw_rx) = mpsc::channel::<()>();

    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = raw_tx.send(());
        }
    }) {
        Ok(w) => w,
        Err(_) => return None,
    };

    if watcher.watch(watch_dir, RecursiveMode::NonRecursive).is_err() {
        return None;
    }

    // Debounce thread: coalesce bursts into one FilesChanged after DEBOUNCE
    // of quiet.
    thread::spawn(move || {
        loop {
            // Block until the first event of a burst.
            if raw_rx.recv().is_err() {
                break;
            }
            // Drain subsequent events until things go quiet for DEBOUNCE.
            loop {
                match raw_rx.recv_timeout(DEBOUNCE) {
                    Ok(()) => continue, // still bursting; keep draining
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
            // Quiet for DEBOUNCE: emit a single coalesced event.
            if tx.send(AppEvent::FilesChanged).is_err() {
                break;
            }
        }
    });

    Some(watcher)
}
