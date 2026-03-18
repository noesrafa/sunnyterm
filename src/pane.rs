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
    pub cursor_renderer: CursorRenderer,
}

impl Pane {
    pub fn new(shell: &str, cols: usize, rows: usize, cursor_blink: bool) -> Self {
        Self {
            grid: Grid::new(cols, rows),
            parser: TermParser::new(),
            pty: Pty::spawn(shell, cols as u16, rows as u16).expect("Failed to spawn PTY"),
            text_renderer: TextRenderer::new(),
            cursor_renderer: CursorRenderer::new(cursor_blink),
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
}
