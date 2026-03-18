use crate::terminal::cell::{CellAttrs, CellColor};
use crate::terminal::grid::Grid;

/// Wraps vte::Parser and processes escape sequences into grid operations.
pub struct TermParser {
    parser: vte::Parser,
}

impl TermParser {
    pub fn new() -> Self {
        Self {
            parser: vte::Parser::new(),
        }
    }

    pub fn process(&mut self, bytes: &[u8], grid: &mut Grid) {
        let mut performer = Performer { grid };
        for byte in bytes {
            self.parser.advance(&mut performer, *byte);
        }
    }
}

struct Performer<'a> {
    grid: &'a mut Grid,
}

impl<'a> vte::Perform for Performer<'a> {
    fn print(&mut self, c: char) {
        self.grid.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // BEL
            0x07 => {}
            // Backspace
            0x08 => self.grid.backspace(),
            // Horizontal tab
            0x09 => self.grid.tab(),
            // Line feed / Vertical tab / Form feed
            0x0A | 0x0B | 0x0C => self.grid.line_feed(),
            // Carriage return
            0x0D => self.grid.carriage_return(),
            _ => {}
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let params: Vec<u16> = params.iter().flat_map(|p| p.iter().copied()).collect();
        let arg0 = params.first().copied().unwrap_or(0);
        let arg1 = params.get(1).copied().unwrap_or(0);

        // Private mode set/reset (CSI ? Ps h / CSI ? Ps l)
        let is_private = intermediates.contains(&b'?');
        if is_private {
            if action == 'h' || action == 'l' {
                let enable = action == 'h';
                for &p in &params {
                    match p {
                        47 | 1047 | 1049 => {
                            self.grid.alternate_screen = enable;
                        }
                        _ => {}
                    }
                }
                self.grid.dirty = true;
            }
            return;
        }

        match action {
            // Cursor Up
            'A' => {
                let n = if arg0 == 0 { 1 } else { arg0 as usize };
                self.grid.cursor_row = self.grid.cursor_row.saturating_sub(n);
            }
            // Cursor Down
            'B' => {
                let n = if arg0 == 0 { 1 } else { arg0 as usize };
                self.grid.cursor_row = (self.grid.cursor_row + n).min(self.grid.rows - 1);
            }
            // Cursor Forward
            'C' => {
                let n = if arg0 == 0 { 1 } else { arg0 as usize };
                self.grid.cursor_col = (self.grid.cursor_col + n).min(self.grid.cols - 1);
            }
            // Cursor Backward
            'D' => {
                let n = if arg0 == 0 { 1 } else { arg0 as usize };
                self.grid.cursor_col = self.grid.cursor_col.saturating_sub(n);
            }
            // Cursor Position (CUP)
            'H' | 'f' => {
                let row = if arg0 == 0 { 1 } else { arg0 as usize };
                let col = if arg1 == 0 { 1 } else { arg1 as usize };
                self.grid.cursor_row = (row - 1).min(self.grid.rows - 1);
                self.grid.cursor_col = (col - 1).min(self.grid.cols - 1);
            }
            // Erase in Display
            'J' => self.grid.erase_in_display(arg0),
            // Erase in Line
            'K' => self.grid.erase_in_line(arg0),
            // Insert Lines
            'L' => {
                let n = if arg0 == 0 { 1 } else { arg0 as usize };
                self.grid.insert_lines(n);
            }
            // Delete Lines
            'M' => {
                let n = if arg0 == 0 { 1 } else { arg0 as usize };
                self.grid.delete_lines(n);
            }
            // Erase Characters
            'X' => {
                let n = if arg0 == 0 { 1 } else { arg0 as usize };
                self.grid.erase_chars(n);
            }
            // Cursor to column
            'G' => {
                let col = if arg0 == 0 { 1 } else { arg0 as usize };
                self.grid.cursor_col = (col - 1).min(self.grid.cols - 1);
            }
            // Cursor to row
            'd' => {
                let row = if arg0 == 0 { 1 } else { arg0 as usize };
                self.grid.cursor_row = (row - 1).min(self.grid.rows - 1);
            }
            // Set Scroll Region
            'r' => {
                let top = if arg0 == 0 { 1 } else { arg0 as usize };
                let bottom = if arg1 == 0 { self.grid.rows } else { arg1 as usize };
                self.grid.scroll_top = (top - 1).min(self.grid.rows - 1);
                self.grid.scroll_bottom = (bottom - 1).min(self.grid.rows - 1);
                self.grid.cursor_row = 0;
                self.grid.cursor_col = 0;
            }
            // SGR - Select Graphic Rendition
            'm' => self.handle_sgr(&params),
            _ => {}
        }
        self.grid.dirty = true;
    }
}

impl<'a> Performer<'a> {
    fn handle_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.grid.attrs = CellAttrs::default();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.grid.attrs = CellAttrs::default(),
                1 => self.grid.attrs.bold = true,
                3 => self.grid.attrs.italic = true,
                4 => self.grid.attrs.underline = true,
                7 => self.grid.attrs.inverse = true,
                22 => self.grid.attrs.bold = false,
                23 => self.grid.attrs.italic = false,
                24 => self.grid.attrs.underline = false,
                27 => self.grid.attrs.inverse = false,

                // Foreground colors
                30..=37 => self.grid.attrs.fg = CellColor::Indexed((params[i] - 30) as u8),
                39 => self.grid.attrs.fg = CellColor::Default,
                90..=97 => self.grid.attrs.fg = CellColor::Indexed((params[i] - 90 + 8) as u8),

                // Background colors
                40..=47 => self.grid.attrs.bg = CellColor::Indexed((params[i] - 40) as u8),
                49 => self.grid.attrs.bg = CellColor::Default,
                100..=107 => self.grid.attrs.bg = CellColor::Indexed((params[i] - 100 + 8) as u8),

                // 256 color / true color foreground
                38 => {
                    if i + 1 < params.len() {
                        match params[i + 1] {
                            5 if i + 2 < params.len() => {
                                self.grid.attrs.fg = CellColor::Indexed(params[i + 2] as u8);
                                i += 2;
                            }
                            2 if i + 4 < params.len() => {
                                self.grid.attrs.fg = CellColor::Rgb(
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                i += 4;
                            }
                            _ => { i += 1; }
                        }
                    }
                }
                // 256 color / true color background
                48 => {
                    if i + 1 < params.len() {
                        match params[i + 1] {
                            5 if i + 2 < params.len() => {
                                self.grid.attrs.bg = CellColor::Indexed(params[i + 2] as u8);
                                i += 2;
                            }
                            2 if i + 4 < params.len() => {
                                self.grid.attrs.bg = CellColor::Rgb(
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                i += 4;
                            }
                            _ => { i += 1; }
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
}
