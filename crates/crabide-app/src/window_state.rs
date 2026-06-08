//! Window state persistence — save/restore window size, position, and maximized state.
//!
//! State is stored as JSON in `~/.crabide/window_state.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
pub fn state_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".crabide").join("window_state.json"))
}

/// Load saved window state, or return defaults if the file doesn't exist or is corrupt.
pub fn load() -> WindowState {
    let path = match state_path() {
        Some(p) => p,
        None => return WindowState::default(),
    };
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => WindowState::default(),
    }
}

/// Save window state to disk. Creates parent directories if needed.
pub fn save(state: &WindowState) {
    let path = match state_path() {
        Some(p) => p,
        None => return,
    };
    let json = match serde_json::to_string_pretty(state) {
        Ok(j) => j,
        Err(e) => {
            log::error!("Failed to serialize window state: {e}");
            return;
        }
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::error!("Failed to create config dir for window state: {e}");
            return;
        }
    }
    if let Err(e) = std::fs::write(&path, json) {
        log::error!("Failed to write window state: {e}");
    }
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
