//! PTY spawn and I/O bridge.
//!
//! Each PTY is driven by two blocking threads (via Tokio's `spawn_blocking`):
//!   - **reader**: drains PTY stdout → feeds `Grid` → takes delta →
//!     sends `TerminalEvent::Output` to the UI event bus.
//!   - **writer**: drains `input_rx` → writes bytes to PTY stdin.
//!
//! portable-pty is a blocking API, so we use `spawn_blocking` instead of
//! async tasks.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crabide_core::event::{EditorEvent, TerminalEvent};
use crossbeam_channel::Sender;

use crate::grid::Grid;

// ── PtyHandle ─────────────────────────────────────────────────────────────────

/// Handle to a running PTY instance.
pub struct PtyHandle {
    pub id: u32,
    /// Send raw bytes to the PTY's stdin (keyboard input).
    pub input_tx: UnboundedSender<Vec<u8>>,
    /// Signal a PTY resize (cols, rows).
    pub resize_tx: UnboundedSender<(u16, u16)>,
}

impl PtyHandle {
    pub fn write(&self, bytes: Vec<u8>) {
        let _ = self.input_tx.send(bytes);
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.resize_tx.send((cols, rows));
    }
}

// ── Spawn ─────────────────────────────────────────────────────────────────────

/// Spawn a new PTY running the OS default shell.
pub fn spawn_pty(
    id: u32,
    cols: u16,
    rows: u16,
    cwd: Option<PathBuf>,
    rt: &Handle,
    event_tx: Sender<EditorEvent>,
) -> Option<PtyHandle> {
    let pty_system = NativePtySystem::default();
    let size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    let pair = pty_system
        .openpty(size)
        .map_err(|e| log::error!("PTY open failed: {e}"))
        .ok()?;

    let shell = default_shell();
    let mut cmd = CommandBuilder::new(&shell);
    if let Some(dir) = &cwd {
        cmd.cwd(dir);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    let _child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| log::error!("shell spawn failed ({shell}): {e}"))
        .ok()?;

    drop(pair.slave);

    // Acquire writer BEFORE wrapping master in Arc so we can move it separately.
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| log::error!("PTY take_writer failed: {e}"))
        .ok()?;

    let master = Arc::new(Mutex::new(pair.master));

    let (input_tx, input_rx) = unbounded_channel::<Vec<u8>>();
    let (resize_tx, resize_rx) = unbounded_channel::<(u16, u16)>();

    // Reader task
    {
        let master_r = Arc::clone(&master);
        let evt_tx = event_tx.clone();
        rt.spawn_blocking(move || pty_reader_loop(id, master_r, evt_tx));
    }

    // Writer task
    rt.spawn_blocking(move || pty_writer_loop(writer, input_rx, resize_rx, master));

    Some(PtyHandle {
        id,
        input_tx,
        resize_tx,
    })
}

// ── Reader loop ───────────────────────────────────────────────────────────────

fn pty_reader_loop(
    id: u32,
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    event_tx: Sender<EditorEvent>,
) {
    let reader = {
        let guard = master.lock().unwrap();
        guard.try_clone_reader()
    };

    let mut reader = match reader {
        Ok(r) => r,
        Err(e) => {
            log::error!("PTY reader clone failed: {e}");
            return;
        }
    };

    let mut grid = Grid::new(80, 24);
    let mut buf = vec![0u8; 4096];
    let mut prev_title: Option<String> = None;
    let mut prev_cwd: Option<String> = None;
    let mut prev_cmd_started: Option<String> = None;
    let mut prev_cmd_finished: Option<i32> = None;

    loop {
        use std::io::Read;
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                log::debug!("PTY read end (id={id}): {e}");
                break;
            }
        };

        grid.feed(&buf[..n]);

        if grid.title != prev_title {
            prev_title = grid.title.clone();
            if let Some(t) = &prev_title {
                let _ = event_tx.send(EditorEvent::Terminal(TerminalEvent::TitleChanged {
                    terminal_id: id,
                    title: t.clone(),
                }));
            }
        }

        if grid.cwd != prev_cwd {
            prev_cwd = grid.cwd.clone();
            if let Some(c) = &prev_cwd {
                let path = PathBuf::from(c.trim_start_matches("file://"));
                let _ = event_tx.send(EditorEvent::Terminal(TerminalEvent::CwdChanged {
                    terminal_id: id,
                    cwd: path,
                }));
            }
        }

        // OSC 133 shell integration events
        if grid.command_started != prev_cmd_started {
            prev_cmd_started = grid.command_started.clone();
            if let Some(cmd) = &prev_cmd_started {
                let _ = event_tx.send(EditorEvent::Terminal(TerminalEvent::CommandStarted {
                    terminal_id: id,
                    command: cmd.clone(),
                }));
            }
        }
        if grid.command_finished != prev_cmd_finished {
            prev_cmd_finished = grid.command_finished;
            if let Some(code) = prev_cmd_finished {
                let _ = event_tx.send(EditorEvent::Terminal(TerminalEvent::CommandFinished {
                    terminal_id: id,
                    exit_code: code,
                }));
            }
        }

        let delta = grid.take_delta();
        if !delta.rows.is_empty() {
            let _ = event_tx.send(EditorEvent::Terminal(TerminalEvent::Output {
                terminal_id: id,
                delta,
            }));
        }
    }

    let _ = event_tx.send(EditorEvent::Terminal(TerminalEvent::Exited {
        terminal_id: id,
        code: None,
    }));
}

// ── Writer loop ───────────────────────────────────────────────────────────────

fn pty_writer_loop(
    mut writer: Box<dyn std::io::Write + Send>,
    mut input_rx: UnboundedReceiver<Vec<u8>>,
    mut resize_rx: UnboundedReceiver<(u16, u16)>,
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
) {
    use std::io::Write;
    loop {
        // Drain resize signals (non-blocking).
        while let Ok((cols, rows)) = resize_rx.try_recv() {
            if let Ok(guard) = master.lock() {
                let _ = guard.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
        }

        match input_rx.blocking_recv() {
            Some(bytes) => {
                let _ = writer.write_all(&bytes);
            }
            None => break,
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_owned())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_owned())
    }
}
