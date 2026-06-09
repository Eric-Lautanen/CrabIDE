//! Terminal instance manager — owns all running PTY handles.
//!
//! `TerminalManager` is held by the app struct.  The UI sets
//! pending flags in `TerminalPanelState`; the app drains them each frame
//! via `TerminalManager::drain_pending`.

use std::path::PathBuf;

use crossbeam_channel::Sender;
use tokio::runtime::Handle;

use crabide_core::event::EditorEvent;

use crabide_core::event::TerminalColorScheme;

use crate::pty::{PtyHandle, spawn_pty};

// ── TerminalProfile ───────────────────────────────────────────────────────────

/// Configuration for a terminal instance.
#[derive(Clone)]
pub struct TerminalProfile {
    /// Override the shell binary (None = OS default).
    pub shell: Option<String>,
    /// Extra environment variables.
    pub env: Vec<(String, String)>,
    /// Override font size (None = inherit from editor).
    pub font_size: Option<f32>,
    /// Color scheme / theme for this terminal.
    pub color_scheme: TerminalColorScheme,
}

impl Default for TerminalProfile {
    fn default() -> Self {
        Self {
            shell: None,
            env: Vec::new(),
            font_size: None,
            color_scheme: TerminalColorScheme::dark(),
        }
    }
}

// ── TerminalManager ───────────────────────────────────────────────────────────

/// Owns all PTY handles and spawns new terminals on request.
pub struct TerminalManager {
    next_id: u32,
    handles: Vec<PtyHandle>,
    event_tx: Sender<EditorEvent>,
    rt: Handle,
}

impl TerminalManager {
    pub fn new(event_tx: Sender<EditorEvent>, rt: Handle) -> Self {
        Self {
            next_id: 1,
            handles: Vec::new(),
            event_tx,
            rt,
        }
    }

    /// Spawn a new PTY.  Returns the terminal ID on success.
    pub fn new_terminal(
        &mut self,
        cols: u16,
        rows: u16,
        cwd: Option<PathBuf>,
        _profile: &TerminalProfile,
    ) -> Option<u32> {
        let id = self.next_id;
        self.next_id += 1;

        let handle = spawn_pty(id, cols, rows, cwd, &self.rt, self.event_tx.clone())?;
        self.handles.push(handle);
        Some(id)
    }

    /// Send keyboard input bytes to terminal `id`.
    pub fn write_input(&self, id: u32, bytes: Vec<u8>) {
        if let Some(h) = self.handle_for(id) {
            h.write(bytes);
        }
    }

    /// Resize terminal `id` to `(cols, rows)`.
    pub fn resize(&self, id: u32, cols: u16, rows: u16) {
        if let Some(h) = self.handle_for(id) {
            h.resize(cols, rows);
        }
    }

    /// Kill terminal `id` by terminating its child process and removing the handle.
    pub fn kill(&mut self, id: u32) {
        if let Some(pos) = self.handles.iter().position(|h| h.id == id) {
            let mut handle = self.handles.swap_remove(pos);
            handle.kill_child();
        }
    }

    fn handle_for(&self, id: u32) -> Option<&PtyHandle> {
        self.handles.iter().find(|h| h.id == id)
    }
}
