use crate::ui::palette;
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
        use palette::dark::*;
        Self {
            clear_color: CANVAS_CLEAR,
            tile_bar: Color::from_hex(BASE),
            tile_border: Color::from_hex(OVERLAY),
            title_focused: Color::from_hex(TEXT),
            title_unfocused: Color::from_hex(TEXT_MUTED),
            btn_bg: Color::from_hex(BASE),
            btn_border: Color::from_hex(OVERLAY),
            icon: Color::from_hex(TEXT),
            label: Color::from_hex(TEXT_MUTED),
            dot_dim_alpha: DOT_DIM_ALPHA,
            dot_bright_alpha: DOT_BRIGHT_ALPHA,
            dot_rgb: DOT_RGB,
            input_bar_bg: Color::from_hex(SURFACE),
            input_bar_border: Color::from_hex(SUBTLE),
        }
    }

    pub fn light() -> Self {
        use palette::light::*;
        Self {
            clear_color: CANVAS_CLEAR,
            tile_bar: Color::from_hex(BASE),
            tile_border: Color::from_hex(OVERLAY),
            title_focused: Color::from_hex(TEXT_DIM),
            title_unfocused: Color::from_hex(TEXT_MUTED),
            btn_bg: Color::from_hex(BUTTONS),
            btn_border: Color::from_hex(OVERLAY),
            icon: Color::from_hex(TEXT),
            label: Color::from_hex(LABEL),
            dot_dim_alpha: DOT_DIM_ALPHA,
            dot_bright_alpha: DOT_BRIGHT_ALPHA,
            dot_rgb: DOT_RGB,
            input_bar_bg: Color::from_hex(SURFACE),
            input_bar_border: Color::from_hex(SUBTLE),
        }
    }
}
