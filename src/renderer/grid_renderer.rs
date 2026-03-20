use crate::renderer::draw_helpers::push_rounded_quad;
use crate::renderer::text::TextVertex;
use crate::ui::canvas_theme::CanvasTheme;

pub fn build_dot_grid(
    pan: (f32, f32),
    view_w: f32,
    view_h: f32,
    scale_factor: f32,
    canvas_theme: &CanvasTheme,
    cursor_pos: (f32, f32),
) -> (Vec<TextVertex>, Vec<u32>) {
    let s = scale_factor;
    let mut dot_v: Vec<TextVertex> = Vec::new();
    let mut dot_i: Vec<u32> = Vec::new();

    let dot_spacing = 24.0 * s;
    let dot_small = 2.8 * s;
    let dot_large = 4.0 * s;
    let base_dim_alpha = canvas_theme.dot_dim_alpha * 0.45;
    let base_bright_alpha = canvas_theme.dot_bright_alpha * 0.45;
    let glow_radius = 260.0 * s; // radius of the cursor glow effect
    let glow_radius_sq = glow_radius * glow_radius;
    let major = 6; // every 6th dot is brighter
    let start_x = (pan.0 / dot_spacing).floor() * dot_spacing;
    let start_y = (pan.1 / dot_spacing).floor() * dot_spacing;
    let (cx, cy) = cursor_pos;
    let mut gx = start_x;
    let mut ix: i32 = ((start_x / dot_spacing).round()) as i32;
    while gx < pan.0 + view_w + dot_spacing {
        let mut gy = start_y;
        let mut iy: i32 = ((start_y / dot_spacing).round()) as i32;
        while gy < pan.1 + view_h + dot_spacing {
            let is_major = ix.rem_euclid(major) == 0 && iy.rem_euclid(major) == 0;

            // Calculate distance from cursor for radial glow
            let dx = gx - cx;
            let dy = gy - cy;
            let dist_sq = dx * dx + dy * dy;

            let glow = if dist_sq < glow_radius_sq {
                let t = 1.0 - (dist_sq / glow_radius_sq).sqrt();
                t * t // quadratic falloff for smooth glow
            } else {
                0.0
            };

            let base_alpha = if is_major { base_bright_alpha } else { base_dim_alpha };
            let max_alpha = if is_major { canvas_theme.dot_bright_alpha * 2.2 } else { canvas_theme.dot_dim_alpha * 3.0 };
            let alpha = (base_alpha + glow * (max_alpha - base_alpha)).min(1.0);

            // Dots near cursor get bigger
            let size_boost = glow * 0.8;
            let base_sz = if is_major { dot_large } else { dot_small };
            let sz = base_sz * (1.0 + size_boost);

            let col = [canvas_theme.dot_rgb, canvas_theme.dot_rgb, canvas_theme.dot_rgb, alpha];
            push_rounded_quad(&mut dot_v, &mut dot_i, gx - sz * 0.5, gy - sz * 0.5, sz, sz, sz, sz, sz * 0.5, col);
            gy += dot_spacing;
            iy += 1;
        }
        gx += dot_spacing;
        ix += 1;
    }

    (dot_v, dot_i)
}
