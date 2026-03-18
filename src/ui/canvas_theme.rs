use crate::ui::theme::Color;

pub struct CanvasTheme {
    pub clear_color: wgpu::Color,
    pub tile_bar: Color,
    pub tile_border: Color,
    pub title_focused: Color,
    pub title_unfocused: Color,
    pub btn_bg: Color,
    pub btn_border: Color,
    pub icon: Color,
    pub label: Color,
    pub dot_dim_alpha: f32,
    pub dot_bright_alpha: f32,
    pub dot_rgb: f32,
    pub input_bar_bg: Color,
    pub input_bar_border: Color,
}

impl CanvasTheme {
    pub fn dark() -> Self {
        Self {
            clear_color: wgpu::Color { r: 0.0024, g: 0.0024, b: 0.0024, a: 1.0 },
            tile_bar: Color::from_hex(0x1B1D1F),
            tile_border: Color::from_hex(0x353B40),
            title_focused: Color::from_hex(0x888888),
            title_unfocused: Color::from_hex(0x555555),
            btn_bg: Color::from_hex(0x1B1D1F),
            btn_border: Color::from_hex(0x353B40),
            icon: Color::from_hex(0x888888),
            label: Color::from_hex(0x555555),
            dot_dim_alpha: 0.15,
            dot_bright_alpha: 0.35,
            dot_rgb: 1.0,
            input_bar_bg: Color::from_hex(0x232629),
            input_bar_border: Color::from_hex(0x3A4048),
        }
    }

    pub fn light() -> Self {
        Self {
            clear_color: wgpu::Color { r: 0.5647, g: 0.5457, b: 0.4452, a: 1.0 },
            tile_bar: Color::from_hex(0xF5F5F5),
            tile_border: Color::from_hex(0xD6CEC4),
            title_focused: Color::from_hex(0x444444),
            title_unfocused: Color::from_hex(0x999999),
            btn_bg: Color::from_hex(0xF5F5F5),
            btn_border: Color::from_hex(0xD6CEC4),
            icon: Color::from_hex(0x555555),
            label: Color::from_hex(0x888888),
            dot_dim_alpha: 0.25,
            dot_bright_alpha: 0.50,
            dot_rgb: 0.0,
            input_bar_bg: Color::from_hex(0xEAEAEA),
            input_bar_border: Color::from_hex(0xC8C0B6),
        }
    }
}
