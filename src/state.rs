use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persistent state saved to ~/.sunnyterm/
#[derive(Debug, Serialize, Deserialize)]
pub struct AppState {
    pub canvas_zoom: f32,
    pub canvas_pan: (f32, f32),
    pub is_dark: bool,
    pub tiles: Vec<TileState>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TileState {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub name: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            canvas_zoom: 1.0,
            canvas_pan: (0.0, 0.0),
            is_dark: true,
            tiles: Vec::new(),
        }
    }
}

impl AppState {
    /// Ensure ~/.sunnyterm/ exists and return the directory path.
    pub fn data_dir() -> PathBuf {
        let dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".sunnyterm");
        if !dir.exists() {
            let _ = std::fs::create_dir_all(&dir);
        }
        dir
    }

    fn state_path() -> PathBuf {
        Self::data_dir().join("state.json")
    }

    pub fn load() -> Self {
        let path = Self::state_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => match serde_json::from_str(&contents) {
                    Ok(state) => return state,
                    Err(e) => eprintln!("[sunnyterm] failed to parse state: {e}"),
                },
                Err(e) => eprintln!("[sunnyterm] failed to read state: {e}"),
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::state_path();
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    eprintln!("[sunnyterm] failed to save state: {e}");
                }
            }
            Err(e) => eprintln!("[sunnyterm] failed to serialize state: {e}"),
        }
    }
}
