use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandHistory {
    entries: Vec<String>,
    #[serde(skip)]
    max_entries: usize,
}

impl CommandHistory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 1000,
        }
    }

    pub fn push(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() { return; }
        // Remove duplicate if it exists
        self.entries.retain(|e| e != cmd);
        self.entries.push(cmd.to_string());
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    /// Get entry by index (0 = most recent).
    pub fn get(&self, index: usize) -> Option<&str> {
        if index >= self.entries.len() { return None; }
        let pos = self.entries.len() - 1 - index;
        Some(&self.entries[pos])
    }

    /// Search entries whose text starts with `prefix` (most recent first).
    pub fn search_prefix(&self, prefix: &str) -> Vec<&str> {
        if prefix.is_empty() { return Vec::new(); }
        let mut results: Vec<&str> = self.entries.iter()
            .rev()
            .filter(|e| e.starts_with(prefix) && e.as_str() != prefix)
            .map(|e| e.as_str())
            .collect();
        // Deduplicate (already deduped by push, but just in case)
        results.dedup();
        results.truncate(50);
        results
    }

    fn path() -> PathBuf {
        AppState::data_dir().join("history.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(mut hist) = serde_json::from_str::<CommandHistory>(&contents) {
                    hist.max_entries = 1000;
                    return hist;
                }
            }
        }
        Self::new()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Ok(json) = serde_json::to_string(self) {
            let _ = std::fs::write(&path, json);
        }
    }
}
