use crate::terminal::cell::{Cell, CellAttrs};
use crate::terminal::scroll::ScrollBuffer;

/// The visible grid of characters plus cursor state.
pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Vec<Cell>>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub attrs: CellAttrs,
    /// Scroll region top (inclusive)
    pub scroll_top: usize,
    /// Scroll region bottom (inclusive)
    pub scroll_bottom: usize,
    pub dirty: bool,
    pub scrollback: ScrollBuffer,
    /// How many lines we're scrolled back (0 = at bottom / live)
    pub scroll_offset: usize,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        Self {
            cols,
            rows,
            cells,
            cursor_row: 0,
            cursor_col: 0,
            attrs: CellAttrs::default(),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            dirty: true,
            scrollback: ScrollBuffer::new(10_000),
            scroll_offset: 0,
        }
    }

    pub fn put_char(&mut self, c: char) {
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.line_feed();
        }
        if self.cursor_row < self.rows && self.cursor_col < self.cols {
            self.cells[self.cursor_row][self.cursor_col] = Cell {
                c,
                attrs: self.attrs,
                width: 1,
            };
            self.cursor_col += 1;
            self.dirty = true;
        }
    }

    pub fn line_feed(&mut self) {
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cursor_row < self.rows - 1 {
            self.cursor_row += 1;
        }
        self.dirty = true;
    }

    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    pub fn tab(&mut self) {
        let next_tab = (self.cursor_col / 8 + 1) * 8;
        self.cursor_col = next_tab.min(self.cols - 1);
    }

    pub fn scroll_up(&mut self, count: usize) {
        for _ in 0..count {
            // Save the line being scrolled off to scrollback (only if scrolling from top)
            if self.scroll_top == 0 {
                let line = self.cells.remove(0);
                self.scrollback.push(line);
            } else {
                self.cells.remove(self.scroll_top);
            }
            self.cells.insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
        }
        // Snap to bottom on new output
        self.scroll_offset = 0;
        self.dirty = true;
    }

    pub fn scroll_down(&mut self, count: usize) {
        for _ in 0..count {
            self.cells.remove(self.scroll_bottom);
            self.cells.insert(self.scroll_top, vec![Cell::default(); self.cols]);
        }
        self.dirty = true;
    }

    pub fn erase_in_display(&mut self, mode: u16) {
        match mode {
            // From cursor to end
            0 => {
                self.erase_in_line(0);
                for row in (self.cursor_row + 1)..self.rows {
                    self.cells[row] = vec![Cell::default(); self.cols];
                }
            }
            // From start to cursor
            1 => {
                for row in 0..self.cursor_row {
                    self.cells[row] = vec![Cell::default(); self.cols];
                }
                self.erase_in_line(1);
            }
            // Entire screen
            2 | 3 => {
                for row in 0..self.rows {
                    self.cells[row] = vec![Cell::default(); self.cols];
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    pub fn erase_in_line(&mut self, mode: u16) {
        let row = self.cursor_row;
        if row >= self.rows { return; }
        match mode {
            0 => {
                for col in self.cursor_col..self.cols {
                    self.cells[row][col] = Cell::default();
                }
            }
            1 => {
                for col in 0..=self.cursor_col.min(self.cols - 1) {
                    self.cells[row][col] = Cell::default();
                }
            }
            2 => {
                self.cells[row] = vec![Cell::default(); self.cols];
            }
            _ => {}
        }
        self.dirty = true;
    }

    pub fn insert_lines(&mut self, count: usize) {
        for _ in 0..count {
            if self.cursor_row <= self.scroll_bottom {
                self.cells.remove(self.scroll_bottom);
                self.cells.insert(self.cursor_row, vec![Cell::default(); self.cols]);
            }
        }
        self.dirty = true;
    }

    pub fn delete_lines(&mut self, count: usize) {
        for _ in 0..count {
            if self.cursor_row <= self.scroll_bottom {
                self.cells.remove(self.cursor_row);
                self.cells.insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
            }
        }
        self.dirty = true;
    }

    pub fn erase_chars(&mut self, count: usize) {
        let row = self.cursor_row;
        for col in self.cursor_col..(self.cursor_col + count).min(self.cols) {
            self.cells[row][col] = Cell::default();
        }
        self.dirty = true;
    }

    /// Scroll the viewport by delta lines (positive = scroll up into history).
    pub fn scroll_viewport(&mut self, delta: i32) {
        let max = self.scrollback.len();
        if delta > 0 {
            self.scroll_offset = (self.scroll_offset + delta as usize).min(max);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as usize);
        }
        self.dirty = true;
    }

    /// Get the line to display at a given row, accounting for scroll_offset.
    /// Returns None if the row should show from the live grid.
    pub fn display_line(&self, row: usize) -> &[Cell] {
        if self.scroll_offset == 0 {
            return &self.cells[row];
        }

        let sb_len = self.scrollback.len();
        // Total virtual lines = scrollback + grid rows
        // We want to show lines ending at (sb_len + rows - scroll_offset)
        let first_visible = sb_len.saturating_sub(self.scroll_offset);
        let line_idx = first_visible + row;

        if line_idx < sb_len {
            if let Some(line) = self.scrollback.get(line_idx) {
                return line;
            }
        }
        // It's a grid line
        let grid_row = line_idx.saturating_sub(sb_len);
        if grid_row < self.rows {
            &self.cells[grid_row]
        } else {
            &self.cells[self.rows - 1]
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        self.cells.resize(rows, vec![Cell::default(); cols]);
        for row in &mut self.cells {
            row.resize(cols, Cell::default());
        }
        self.scroll_bottom = rows.saturating_sub(1);
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
        self.dirty = true;
    }
}
