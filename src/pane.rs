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
        }
    }

    pub fn read_pty(&mut self) {
        let data = self.pty.try_read();
        if !data.is_empty() {
            self.parser.process(&data, &mut self.grid);
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        if cols > 0 && rows > 0 && (cols != self.grid.cols || rows != self.grid.rows) {
            self.grid.resize(cols, rows);
            let _ = self.pty.resize(cols as u16, rows as u16);
        }
    }

    /// Submit the input buffer as a command to the PTY.
    pub fn submit_input(&mut self) {
        let mut cmd = self.input_buffer.clone();
        cmd.push('\r');
        let _ = self.pty.write(cmd.as_bytes());
        self.input_buffer.clear();
        self.input_cursor = 0;
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

    /// Get cursor column (char count, not byte offset).
    pub fn input_cursor_col(&self) -> usize {
        self.input_buffer[..self.input_cursor].chars().count()
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
}
