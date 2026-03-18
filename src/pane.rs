use crate::input::completion::CompletionState;
use crate::renderer::cursor::CursorRenderer;
use crate::renderer::text::TextRenderer;
use crate::terminal::grid::Grid;
use crate::terminal::parser::TermParser;
use crate::terminal::pty::Pty;

pub struct Pane {
    pub grid: Grid,
    pub parser: TermParser,
    pub pty: Pty,
    pub text_renderer: TextRenderer,
    pub input_renderer: TextRenderer,
    pub cursor_renderer: CursorRenderer,
    pub input_buffer: String,
    pub input_cursor: usize, // byte offset in input_buffer
    /// True when a child process is running in the foreground (e.g. claude, node).
    /// Input bypasses the buffer and goes directly to the PTY.
    pub passthrough: bool,
    /// Scroll offset for multiline input (lines scrolled up).
    pub input_scroll: usize,
    /// Active tab completion session.
    pub completion: Option<CompletionState>,
    /// History navigation index (0 = most recent). None = not navigating.
    pub history_index: Option<usize>,
    /// Stashed input buffer when entering history navigation.
    pub history_stash: String,
}

impl Pane {
    pub fn new(shell: &str, cols: usize, rows: usize, cursor_blink: bool) -> Self {
        Self {
            grid: Grid::new(cols, rows),
            parser: TermParser::new(),
            pty: Pty::spawn(shell, cols as u16, rows as u16).expect("Failed to spawn PTY"),
            text_renderer: TextRenderer::new(),
            input_renderer: TextRenderer::new(),
            cursor_renderer: CursorRenderer::new(cursor_blink),
            input_buffer: String::new(),
            input_cursor: 0,
            passthrough: false,
            input_scroll: 0,
            completion: None,
            history_index: None,
            history_stash: String::new(),
        }
    }

    pub fn read_pty(&mut self) {
        let data = self.pty.try_read();
        if !data.is_empty() {
            self.parser.process(&data, &mut self.grid);
        }
        self.passthrough = !self.grid.alternate_screen && self.pty.has_foreground_child();
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        if cols > 0 && rows > 0 && (cols != self.grid.cols || rows != self.grid.rows) {
            self.grid.resize(cols, rows);
            let _ = self.pty.resize(cols as u16, rows as u16);
        }
    }

    /// Submit the input buffer as a command to the PTY.
    /// Each line is sent separately with \r.
    pub fn submit_input(&mut self) {
        for (i, line) in self.input_buffer.split('\n').enumerate() {
            if i > 0 {
                let _ = self.pty.write(b"\r");
            }
            let _ = self.pty.write(line.as_bytes());
        }
        let _ = self.pty.write(b"\r");
        self.input_buffer.clear();
        self.input_cursor = 0;
        self.input_scroll = 0;
    }

    /// Insert text at cursor position.
    pub fn input_insert(&mut self, text: &str) {
        self.input_buffer.insert_str(self.input_cursor, text);
        self.input_cursor += text.len();
    }

    /// Delete char before cursor.
    pub fn input_backspace(&mut self) {
        if self.input_cursor > 0 {
            // Find previous char boundary
            let prev = self.input_buffer[..self.input_cursor]
                .char_indices()
                .rev()
                .next()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input_buffer.drain(prev..self.input_cursor);
            self.input_cursor = prev;
        }
    }

    /// Move cursor left one char.
    pub fn input_move_left(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor = self.input_buffer[..self.input_cursor]
                .char_indices()
                .rev()
                .next()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right one char.
    pub fn input_move_right(&mut self) {
        if self.input_cursor < self.input_buffer.len() {
            self.input_cursor = self.input_buffer[self.input_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.input_cursor + i)
                .unwrap_or(self.input_buffer.len());
        }
    }

    /// Get cursor column (char count, not byte offset) — single-line legacy.
    pub fn input_cursor_col(&self) -> usize {
        self.input_buffer[..self.input_cursor].chars().count()
    }

    /// Get the (row, col) of the cursor in the multiline input buffer.
    pub fn input_cursor_pos(&self) -> (usize, usize) {
        let before = &self.input_buffer[..self.input_cursor];
        let row = before.matches('\n').count();
        let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = before[last_newline..].chars().count();
        (row, col)
    }

    /// Get the lines of the input buffer.
    pub fn input_lines(&self) -> Vec<&str> {
        self.input_buffer.split('\n').collect()
    }

    /// Number of lines in the input buffer.
    pub fn input_line_count(&self) -> usize {
        self.input_buffer.matches('\n').count() + 1
    }

    /// Ensure input_scroll keeps the cursor visible given max_visible_lines.
    pub fn ensure_cursor_visible(&mut self, max_lines: usize) {
        let total = self.input_line_count();
        let max_scroll = total.saturating_sub(max_lines);
        self.input_scroll = self.input_scroll.min(max_scroll);
        let (cursor_row, _) = self.input_cursor_pos();
        if cursor_row < self.input_scroll {
            self.input_scroll = cursor_row;
        } else if cursor_row >= self.input_scroll + max_lines {
            self.input_scroll = cursor_row - max_lines + 1;
        }
    }

    /// Delete char after cursor.
    pub fn input_delete_forward(&mut self) {
        if self.input_cursor < self.input_buffer.len() {
            let next = self.input_buffer[self.input_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.input_cursor + i)
                .unwrap_or(self.input_buffer.len());
            self.input_buffer.drain(self.input_cursor..next);
        }
    }

    /// Delete word backward from cursor.
    pub fn input_delete_word_back(&mut self) {
        if self.input_cursor == 0 { return; }
        let before = &self.input_buffer[..self.input_cursor];
        // Skip trailing spaces
        let trimmed = before.trim_end();
        if trimmed.is_empty() {
            self.input_buffer.drain(..self.input_cursor);
            self.input_cursor = 0;
            return;
        }
        // Find last space/separator
        let word_start = trimmed.rfind(|c: char| c == ' ' || c == '/' || c == '-' || c == '.')
            .map(|i| i + 1)
            .unwrap_or(0);
        self.input_buffer.drain(word_start..self.input_cursor);
        self.input_cursor = word_start;
    }

    /// Move cursor one word left.
    pub fn input_move_word_left(&mut self) {
        if self.input_cursor == 0 { return; }
        let before = &self.input_buffer[..self.input_cursor];
        let trimmed = before.trim_end();
        if trimmed.is_empty() {
            self.input_cursor = 0;
            return;
        }
        self.input_cursor = trimmed.rfind(|c: char| c == ' ' || c == '/' || c == '-' || c == '.')
            .map(|i| i + 1)
            .unwrap_or(0);
    }

    /// Move cursor one word right.
    pub fn input_move_word_right(&mut self) {
        if self.input_cursor >= self.input_buffer.len() { return; }
        let after = &self.input_buffer[self.input_cursor..];
        // Skip current word chars, then skip separators
        let skip_word = after.find(|c: char| c == ' ' || c == '/' || c == '-' || c == '.')
            .unwrap_or(after.len());
        let rest = &after[skip_word..];
        let skip_sep = rest.find(|c: char| c != ' ' && c != '/' && c != '-' && c != '.')
            .unwrap_or(rest.len());
        self.input_cursor += skip_word + skip_sep;
    }

    /// Send Ctrl+C to PTY and clear input.
    pub fn input_interrupt(&mut self) {
        let _ = self.pty.write(b"\x03");
        self.input_buffer.clear();
        self.input_cursor = 0;
    }

    /// Send Ctrl+D to PTY.
    pub fn input_eof(&mut self) {
        if self.input_buffer.is_empty() {
            let _ = self.pty.write(b"\x04");
        }
    }

    /// Apply the current completion candidate to the input buffer.
    pub fn apply_completion(&mut self, index: usize) {
        let Some(ref comp) = self.completion else { return };
        if index >= comp.candidates.len() { return; }
        let candidate = comp.candidates[index].clone();
        let word_start = comp.word_start;
        self.input_buffer.replace_range(word_start..self.input_cursor, &candidate);
        self.input_cursor = word_start + candidate.len();
    }

    /// Cycle to the next completion candidate.
    pub fn cycle_completion(&mut self) {
        let Some(ref mut comp) = self.completion else { return };
        // Revert to original word first
        let original = comp.original_word.clone();
        let word_start = comp.word_start;
        self.input_buffer.replace_range(word_start..self.input_cursor, &original);
        self.input_cursor = word_start + original.len();
        // Advance index
        comp.index = (comp.index + 1) % comp.candidates.len();
        let candidate = comp.candidates[comp.index].clone();
        self.input_buffer.replace_range(word_start..self.input_cursor, &candidate);
        self.input_cursor = word_start + candidate.len();
    }

    /// Cancel any active completion session.
    pub fn cancel_completion(&mut self) {
        self.completion = None;
    }
}
