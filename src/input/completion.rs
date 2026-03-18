use std::path::Path;

use crate::input::history::CommandHistory;

pub struct CompletionState {
    /// The original word before completion started
    pub original_word: String,
    /// Byte offset in the input buffer where the word starts
    pub word_start: usize,
    /// Original cursor position (byte offset)
    pub original_cursor: usize,
    /// All matching candidates (replacement strings)
    pub candidates: Vec<String>,
    /// Current index into candidates
    pub index: usize,
}

/// Extract the word being completed and its start position.
/// Words are delimited by whitespace, pipe, semicolons, &&, ||.
fn extract_word(input: &str, cursor: usize) -> (usize, &str) {
    let before = &input[..cursor];
    // Find the start of the current word
    let word_start = before.rfind(|c: char| c.is_whitespace() || c == '|' || c == ';' || c == '&')
        .map(|i| i + 1)
        .unwrap_or(0);
    (word_start, &before[word_start..])
}

/// Check if this word is a "command position" (first token after start, pipe, ;, &&, ||).
fn is_command_position(input: &str, word_start: usize) -> bool {
    if word_start == 0 { return true; }
    let before = input[..word_start].trim_end();
    if before.is_empty() { return true; }
    let last_char = before.chars().last().unwrap();
    matches!(last_char, '|' | ';' | '&')
}

/// Complete file/directory paths.
fn complete_path(word: &str, cwd: &Path) -> Vec<String> {
    let (dir, prefix) = if word.contains('/') {
        let sep = word.rfind('/').unwrap();
        let dir_part = &word[..=sep];
        let prefix_part = &word[sep + 1..];
        // Expand tilde
        let resolved = if dir_part.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                home.join(&dir_part[2..]) // skip ~/
            } else {
                cwd.join(dir_part)
            }
        } else {
            let p = Path::new(dir_part);
            if p.is_absolute() { p.to_path_buf() } else { cwd.join(dir_part) }
        };
        (resolved, prefix_part.to_string())
    } else if word.starts_with('~') {
        // Just ~ with no slash yet
        if let Some(home) = dirs::home_dir() {
            (home, word[1..].to_string())
        } else {
            return Vec::new();
        }
    } else {
        (cwd.to_path_buf(), word.to_string())
    };

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let prefix_lower = prefix.to_lowercase();
    let mut results: Vec<(String, bool)> = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden files unless prefix starts with dot
        if name.starts_with('.') && !prefix.starts_with('.') {
            continue;
        }
        if name.to_lowercase().starts_with(&prefix_lower) {
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            // Build the full replacement: reuse the user's typed directory prefix
            let replacement = if word.contains('/') {
                let sep = word.rfind('/').unwrap();
                format!("{}{}{}", &word[..=sep], name, if is_dir { "/" } else { "" })
            } else if word.starts_with('~') {
                format!("~/{}{}", name, if is_dir { "/" } else { "" })
            } else {
                format!("{}{}", name, if is_dir { "/" } else { "" })
            };
            results.push((replacement, is_dir));
        }
    }

    // Sort: directories first, then alphabetical
    results.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    results.into_iter().map(|(name, _)| name).take(100).collect()
}

/// Build a completion state for the given input and cursor position.
pub fn complete(input: &str, cursor: usize, cwd: &Path, history: &CommandHistory) -> CompletionState {
    let (word_start, word) = extract_word(input, cursor);

    let mut candidates = Vec::new();

    if word.is_empty() {
        return CompletionState {
            original_word: word.to_string(),
            word_start,
            original_cursor: cursor,
            candidates,
            index: 0,
        };
    }

    // Path completion (if word looks like a path)
    let path_candidates = complete_path(word, cwd);

    if is_command_position(input, word_start) {
        // In command position: history first, then paths
        let hist = history.search_prefix(word);
        // Extract just the first token from history entries for command completion
        for entry in &hist {
            let first_word = entry.split_whitespace().next().unwrap_or(entry);
            if first_word.starts_with(word) && !candidates.contains(&first_word.to_string()) {
                candidates.push(first_word.to_string());
            }
        }
        // Also add full history entries if the whole line matches
        for entry in &hist {
            let s = entry.to_string();
            if !candidates.contains(&s) {
                candidates.push(s);
            }
        }
        // Then path completions
        for p in path_candidates {
            if !candidates.contains(&p) {
                candidates.push(p);
            }
        }
    } else {
        // Not command position: path completion only
        candidates = path_candidates;
    }

    CompletionState {
        original_word: word.to_string(),
        word_start,
        original_cursor: cursor,
        candidates,
        index: 0,
    }
}

/// Find the common prefix among all candidates.
pub fn common_prefix(candidates: &[String]) -> Option<String> {
    if candidates.is_empty() { return None; }
    if candidates.len() == 1 { return Some(candidates[0].clone()); }
    let first = &candidates[0];
    let mut len = first.len();
    for c in &candidates[1..] {
        len = first.chars()
            .zip(c.chars())
            .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
            .count();
        len = first.char_indices()
            .nth(len)
            .map(|(i, _)| i)
            .unwrap_or(first.len().min(c.len()));
    }
    if len > 0 { Some(first[..len].to_string()) } else { None }
}
