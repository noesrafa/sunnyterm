use crate::ui::palette::{self, BLACK, WHITE};

/// Convert sRGB component to linear space
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// RGBA color (0.0 - 1.0, linear space)
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_hex(hex: u32) -> Self {
        Self {
            r: srgb_to_linear(((hex >> 16) & 0xFF) as f32 / 255.0),
            g: srgb_to_linear(((hex >> 8) & 0xFF) as f32 / 255.0),
            b: srgb_to_linear((hex & 0xFF) as f32 / 255.0),
            a: 1.0,
        }
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub cursor: Color,
    pub selection: Color,
    pub ansi: [Color; 16],
}

impl Theme {
    pub fn catppuccin_mocha() -> Self {
        use palette::dark::*;
        Self {
            background: Color::from_hex(BASE),
            foreground: Color::from_hex(TEXT_BRIGHT),
            cursor: WHITE,
            selection: Color::from_hex(SELECTION),
            ansi: [
                Color::from_hex(ANSI_BLACK),
                Color::from_hex(ANSI_RED),
                Color::from_hex(ANSI_GREEN),
                Color::from_hex(ANSI_YELLOW),
                Color::from_hex(ANSI_BLUE),
                Color::from_hex(ANSI_MAGENTA),
                Color::from_hex(ANSI_CYAN),
                Color::from_hex(ANSI_WHITE),
                Color::from_hex(ANSI_BRIGHT_BLACK),
                Color::from_hex(ANSI_RED),
                Color::from_hex(ANSI_GREEN),
                Color::from_hex(ANSI_YELLOW),
                Color::from_hex(ANSI_BLUE),
                Color::from_hex(ANSI_MAGENTA),
                Color::from_hex(ANSI_CYAN),
                Color::from_hex(ANSI_BRIGHT_WHITE),
            ],
        }
    }

    pub fn light() -> Self {
        use palette::light::*;
        Self {
            background: Color::from_hex(BASE),
            foreground: Color::from_hex(TEXT_DARK),
            cursor: BLACK,
            selection: Color::from_hex(SELECTION),
            ansi: [
                Color::from_hex(ANSI_BLACK),
                Color::from_hex(ANSI_RED),
                Color::from_hex(ANSI_GREEN),
                Color::from_hex(ANSI_YELLOW),
                Color::from_hex(ANSI_BLUE),
                Color::from_hex(ANSI_MAGENTA),
                Color::from_hex(ANSI_CYAN),
                Color::from_hex(ANSI_WHITE),
                Color::from_hex(ANSI_BRIGHT_BLACK),
                Color::from_hex(ANSI_BRIGHT_RED),
                Color::from_hex(ANSI_BRIGHT_GREEN),
                Color::from_hex(ANSI_BRIGHT_YELLOW),
                Color::from_hex(ANSI_BRIGHT_BLUE),
                Color::from_hex(ANSI_BRIGHT_MAGENTA),
                Color::from_hex(ANSI_CYAN),
                Color::from_hex(ANSI_BRIGHT_WHITE),
            ],
        }
    }

    pub fn color_from_ansi(&self, index: u8) -> Color {
        if (index as usize) < self.ansi.len() {
            self.ansi[index as usize]
        } else if index < 232 {
            let idx = index - 16;
            let r = srgb_to_linear((idx / 36) as f32 / 5.0);
            let g = srgb_to_linear(((idx % 36) / 6) as f32 / 5.0);
            let b = srgb_to_linear((idx % 6) as f32 / 5.0);
            Color::new(r, g, b, 1.0)
        } else {
            let level = srgb_to_linear((index - 232) as f32 / 23.0);
            Color::new(level, level, level, 1.0)
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::catppuccin_mocha()
    }
}
