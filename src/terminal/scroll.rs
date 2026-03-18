use crate::terminal::cell::Cell;

/// Stores scrollback history lines.
pub struct ScrollBuffer {
    lines: Vec<Vec<Cell>>,
    max_lines: usize,
}

impl ScrollBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: Vec::new(),
            max_lines,
        }
    }

    pub fn push(&mut self, line: Vec<Cell>) {
        self.lines.push(line);
        if self.lines.len() > self.max_lines {
            self.lines.remove(0);
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn get(&self, index: usize) -> Option<&Vec<Cell>> {
        self.lines.get(index)
    }

    pub fn lines(&self) -> &[Vec<Cell>] {
        &self.lines
    }

    pub fn restore(&mut self, lines: Vec<Vec<Cell>>) {
        self.lines = lines;
        // Trim to max if needed
        if self.lines.len() > self.max_lines {
            let excess = self.lines.len() - self.max_lines;
            self.lines.drain(..excess);
        }
    }
}
