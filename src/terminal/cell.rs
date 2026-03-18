use crate::ui::theme::Color;

#[derive(Debug, Clone, Copy)]
pub struct CellAttrs {
    pub fg: CellColor,
    pub bg: CellColor,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum CellColor {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Default for CellAttrs {
    fn default() -> Self {
        Self {
            fg: CellColor::Default,
            bg: CellColor::Default,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
        }
    }
}

impl CellColor {
    pub fn to_color(&self, theme: &crate::ui::theme::Theme, is_fg: bool) -> Color {
        match self {
            CellColor::Default => {
                if is_fg { theme.foreground } else { theme.background }
            }
            CellColor::Indexed(idx) => theme.color_from_ansi(*idx),
            CellColor::Rgb(r, g, b) => Color::from_hex(
                ((*r as u32) << 16) | ((*g as u32) << 8) | (*b as u32)
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub c: char,
    pub attrs: CellAttrs,
    pub width: u8, // 1 for normal, 2 for wide chars
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            attrs: CellAttrs::default(),
            width: 1,
        }
    }
}
