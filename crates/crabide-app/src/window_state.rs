//! Window state persistence — save/restore window size, position, and maximized state.
//!
//! State is stored as JSON in `~/.crabide/window_state.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Window geometry ─────────────────────────────────────────────────────────────

/// Persisted window geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub width: f32,
    pub height: f32,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 800.0,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

/// Path to the window state file in the user config directory.
fn window_state_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("window_state.json"))
}

/// Load saved window state, or return defaults if the file doesn't exist or is corrupt.
pub fn load_window_state() -> WindowState {
    let Some(path) = window_state_path() else {
        return WindowState::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => WindowState::default(),
    }
}

/// Save window state to disk. Creates parent directories if needed.
pub fn save_window_state(state: &WindowState) {
    with_json_file(&window_state_path(), state, "window state");
}

// ── Session (open files) ────────────────────────────────────────────────────────

/// Persisted set of open editor file paths to restore on next launch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    /// File paths that were open when the editor last closed.
    pub open_files: Vec<String>,
}

/// Path to the session state file.
fn session_state_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("session.json"))
}

/// Load session state (list of files open in last session).
pub fn load_session() -> SessionState {
    let Some(path) = session_state_path() else {
        return SessionState::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => SessionState::default(),
    }
}

/// Save session state (list of open file paths).
pub fn save_session(state: &SessionState) {
    with_json_file(&session_state_path(), state, "session");
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

fn config_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".crabide"))
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn with_json_file<T: Serialize>(path: &Option<PathBuf>, value: &T, label: &str) {
    let Some(path) = path else {
        return;
    };
    let json = match serde_json::to_string_pretty(value) {
        Ok(j) => j,
        Err(e) => {
            log::error!("Failed to serialize {label}: {e}");
            return;
        }
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::error!("Failed to create config dir for {label}: {e}");
            return;
        }
    }
    if let Err(e) = std::fs::write(path, json) {
        log::error!("Failed to write {label}: {e}");
    }
}
