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

/// Catppuccin Mocha color palette
#[derive(Debug, Clone)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub cursor: Color,
    pub selection: Color,

    // ANSI colors (0-15)
    pub ansi: [Color; 16],
}

impl Theme {
    pub fn catppuccin_mocha() -> Self {
        Self {
            background: Color::from_hex(0x1B1D1F),
            foreground: Color::from_hex(0xE0E0E0),
            cursor: Color::from_hex(0xFFFFFF),
            selection: Color { ..Color::from_hex(0x585B70) },

            ansi: [
                // Normal colors (0-7)
                Color::from_hex(0x45475A), // black
                Color::from_hex(0xF38BA8), // red
                Color::from_hex(0xA6E3A1), // green
                Color::from_hex(0xF9E2AF), // yellow
                Color::from_hex(0x89B4FA), // blue
                Color::from_hex(0xF5C2E7), // magenta
                Color::from_hex(0x94E2D5), // cyan
                Color::from_hex(0xBAC2DE), // white
                // Bright colors (8-15)
                Color::from_hex(0x585B70), // bright black
                Color::from_hex(0xF38BA8), // bright red
                Color::from_hex(0xA6E3A1), // bright green
                Color::from_hex(0xF9E2AF), // bright yellow
                Color::from_hex(0x89B4FA), // bright blue
                Color::from_hex(0xF5C2E7), // bright magenta
                Color::from_hex(0x94E2D5), // bright cyan
                Color::from_hex(0xA6ADC8), // bright white
            ],
        }
    }

    pub fn light() -> Self {
        Self {
            background: Color::from_hex(0xF5F5F5),
            foreground: Color::from_hex(0x1B1D1F),
            cursor: Color::from_hex(0x000000),
            selection: Color::from_hex(0xB4D5FE),

            ansi: [
                Color::from_hex(0x000000), // black
                Color::from_hex(0xC41A15), // red
                Color::from_hex(0x2EA043), // green
                Color::from_hex(0x9A6700), // yellow
                Color::from_hex(0x0969DA), // blue
                Color::from_hex(0x8250DF), // magenta
                Color::from_hex(0x1B7C83), // cyan
                Color::from_hex(0x6E7781), // white
                Color::from_hex(0x57606A), // bright black
                Color::from_hex(0xCF222E), // bright red
                Color::from_hex(0x116329), // bright green
                Color::from_hex(0x4D2D00), // bright yellow
                Color::from_hex(0x0550AE), // bright blue
                Color::from_hex(0x6639BA), // bright magenta
                Color::from_hex(0x1B7C83), // bright cyan
                Color::from_hex(0x8C959F), // bright white
            ],
        }
    }

    pub fn color_from_ansi(&self, index: u8) -> Color {
        if (index as usize) < self.ansi.len() {
            self.ansi[index as usize]
        } else if index < 232 {
            // 216 color cube (indices 16-231)
            let idx = index - 16;
            let r = srgb_to_linear((idx / 36) as f32 / 5.0);
            let g = srgb_to_linear(((idx % 36) / 6) as f32 / 5.0);
            let b = srgb_to_linear((idx % 6) as f32 / 5.0);
            Color::new(r, g, b, 1.0)
        } else {
            // Grayscale (indices 232-255)
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
