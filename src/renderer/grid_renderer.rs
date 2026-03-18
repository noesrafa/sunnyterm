use crate::renderer::draw_helpers::push_rounded_quad;
use crate::renderer::text::TextVertex;
use crate::ui::canvas_theme::CanvasTheme;

pub fn build_dot_grid(
    pan: (f32, f32),
    view_w: f32,
    view_h: f32,
    scale_factor: f32,
    canvas_theme: &CanvasTheme,
) -> (Vec<TextVertex>, Vec<u32>) {
    let s = scale_factor;
    let mut dot_v: Vec<TextVertex> = Vec::new();
    let mut dot_i: Vec<u32> = Vec::new();

    let dot_spacing = 24.0 * s;
    let dot_small = 2.0 * s;
    let dot_large = 3.2 * s;
    let color_dim = [canvas_theme.dot_rgb, canvas_theme.dot_rgb, canvas_theme.dot_rgb, canvas_theme.dot_dim_alpha];
    let color_bright = [canvas_theme.dot_rgb, canvas_theme.dot_rgb, canvas_theme.dot_rgb, canvas_theme.dot_bright_alpha];
    let major = 6; // every 6th dot is brighter
    let start_x = (pan.0 / dot_spacing).floor() * dot_spacing;
    let start_y = (pan.1 / dot_spacing).floor() * dot_spacing;
    let mut gx = start_x;
    let mut ix: i32 = ((start_x / dot_spacing).round()) as i32;
    while gx < pan.0 + view_w + dot_spacing {
        let mut gy = start_y;
        let mut iy: i32 = ((start_y / dot_spacing).round()) as i32;
        while gy < pan.1 + view_h + dot_spacing {
            let is_major = ix.rem_euclid(major) == 0 && iy.rem_euclid(major) == 0;
            let (sz, col) = if is_major { (dot_large, color_bright) } else { (dot_small, color_dim) };
            push_rounded_quad(&mut dot_v, &mut dot_i, gx - sz * 0.5, gy - sz * 0.5, sz, sz, sz, sz, sz * 0.5, col);
            gy += dot_spacing;
            iy += 1;
        }
        gx += dot_spacing;
        ix += 1;
    }

    (dot_v, dot_i)
}
