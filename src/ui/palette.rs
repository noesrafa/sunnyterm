use crate::ui::theme::Color;

// ── Shared neutrals ──────────────────────────────────────────────
pub const BLACK: Color          = Color::new(0.0, 0.0, 0.0, 1.0);
pub const WHITE: Color          = Color::new(1.0, 1.0, 1.0, 1.0);

// ── Dark palette ─────────────────────────────────────────────────
pub mod dark {


    pub const BASE:           u32 = 0x1B1D1F;
    pub const SURFACE:        u32 = 0x232629;
    pub const OVERLAY:        u32 = 0x353B40;
    pub const SUBTLE:         u32 = 0x3A4048;
    pub const TEXT_MUTED:     u32 = 0x555555;
    pub const TEXT:           u32 = 0x888888;
    pub const TEXT_BRIGHT:    u32 = 0xE0E0E0;

    pub const CANVAS_CLEAR: wgpu::Color = wgpu::Color { r: 0.0024, g: 0.0024, b: 0.0024, a: 1.0 };

    // Catppuccin Mocha ANSI
    pub const ANSI_BLACK:          u32 = 0x45475A;
    pub const ANSI_RED:            u32 = 0xF38BA8;
    pub const ANSI_GREEN:          u32 = 0xA6E3A1;
    pub const ANSI_YELLOW:         u32 = 0xF9E2AF;
    pub const ANSI_BLUE:           u32 = 0x89B4FA;
    pub const ANSI_MAGENTA:        u32 = 0xF5C2E7;
    pub const ANSI_CYAN:           u32 = 0x94E2D5;
    pub const ANSI_WHITE:          u32 = 0xBAC2DE;
    pub const ANSI_BRIGHT_BLACK:   u32 = 0x585B70;
    pub const ANSI_BRIGHT_WHITE:   u32 = 0xA6ADC8;

    pub const SELECTION:           u32 = 0x585B70;

    pub const DOT_DIM_ALPHA:   f32 = 0.15;
    pub const DOT_BRIGHT_ALPHA: f32 = 0.35;
    pub const DOT_RGB:          f32 = 1.0;
}

// ── Light palette ────────────────────────────────────────────────
pub mod light {


    pub const BASE:           u32 = 0xEEEBE1;
    pub const SURFACE:        u32 = 0xEAEAEA;
    pub const OVERLAY:        u32 = 0xD6CEC4;
    pub const SUBTLE:         u32 = 0xC8C0B6;
    pub const TEXT_MUTED:     u32 = 0x999999;
    pub const TEXT:           u32 = 0x555555;
    pub const TEXT_DIM:       u32 = 0x444444;
    pub const TEXT_DARK:      u32 = 0x1B1D1F;
    pub const LABEL:          u32 = 0x9E9882;
    pub const BUTTONS:        u32 = 0xF5F5F5;

    pub const CANVAS_CLEAR: wgpu::Color = wgpu::Color { r: 0.5647, g: 0.5457, b: 0.4452, a: 1.0 };

    pub const SELECTION:           u32 = 0xB4D5FE;

    // ANSI
    pub const ANSI_BLACK:          u32 = 0x000000;
    pub const ANSI_RED:            u32 = 0xC41A15;
    pub const ANSI_GREEN:          u32 = 0x2EA043;
    pub const ANSI_YELLOW:         u32 = 0x9A6700;
    pub const ANSI_BLUE:           u32 = 0x0969DA;
    pub const ANSI_MAGENTA:        u32 = 0x8250DF;
    pub const ANSI_CYAN:           u32 = 0x1B7C83;
    pub const ANSI_WHITE:          u32 = 0x6E7781;
    pub const ANSI_BRIGHT_BLACK:   u32 = 0x57606A;
    pub const ANSI_BRIGHT_RED:     u32 = 0xCF222E;
    pub const ANSI_BRIGHT_GREEN:   u32 = 0x116329;
    pub const ANSI_BRIGHT_YELLOW:  u32 = 0x4D2D00;
    pub const ANSI_BRIGHT_BLUE:    u32 = 0x0550AE;
    pub const ANSI_BRIGHT_MAGENTA: u32 = 0x6639BA;
    pub const ANSI_BRIGHT_WHITE:   u32 = 0x8C959F;

    pub const DOT_DIM_ALPHA:   f32 = 0.25;
    pub const DOT_BRIGHT_ALPHA: f32 = 0.50;
    pub const DOT_RGB:          f32 = 0.0;
}
