use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::draw_helpers::{push_quad, push_rounded_quad, DrawBatch};
use crate::renderer::text::TextVertex;
use crate::ui::canvas_theme::CanvasTheme;

pub fn build_ui_batch(
    zoom: f32,
    pan: (f32, f32),
    is_dark: bool,
    atlas: &mut GlyphAtlas,
    canvas_theme: &CanvasTheme,
    gpu_queue: &wgpu::Queue,
    surface_width: f32,
    surface_height: f32,
    scale_factor: f32,
) -> DrawBatch {
    let s = scale_factor;
    let z = 1.0 / zoom; // scale factor for UI elements
    let sw = surface_width / zoom + pan.0;
    let sh = surface_height / zoom + pan.1;
    let btn_w = 32.0 * s * z;
    let btn_h = 32.0 * s * z;
    let margin = 16.0 * s * z;
    let gap = 4.0 * s * z;
    let radius = 8.0 * s * z;
    let bw = 1.0 * s * z;

    let mut batch = DrawBatch::new();

    let btn_bg = canvas_theme.btn_bg.to_array();
    let btn_border = canvas_theme.btn_border.to_array();
    let icon_color = canvas_theme.icon.to_array();
    let line_w = 1.5 * s * z;

    // Container: both buttons in one rounded pill + toggle below
    let pill_h = btn_h * 2.0 + gap;
    let total_ui_h = pill_h + gap * 2.0 + btn_h; // pill + gap + toggle
    let bx = sw - margin - btn_w;
    let by = sh - margin - total_ui_h;

    // Border (outer rounded rect)
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        bx - bw, by - bw, btn_w + bw * 2.0, pill_h + bw * 2.0,
        btn_w + bw * 2.0, pill_h + bw * 2.0, radius + bw, btn_border);
    // Background (inner rounded rect)
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        bx, by, btn_w, pill_h, btn_w, pill_h, radius, btn_bg);

    // Divider line between buttons
    let div_y = by + btn_h + gap * 0.5 - bw * 0.5;
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices, bx + 6.0 * s * z, div_y, btn_w - 12.0 * s * z, bw, btn_border);

    // + icon (top button)
    let icon_len = 10.0 * s * z;
    // horizontal
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        bx + (btn_w - icon_len) / 2.0, by + (btn_h - line_w) / 2.0,
        icon_len, line_w, icon_color);
    // vertical
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        bx + (btn_w - line_w) / 2.0, by + (btn_h - icon_len) / 2.0,
        line_w, icon_len, icon_color);

    // - icon (bottom button)
    let by2 = by + btn_h + gap;
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        bx + (btn_w - icon_len) / 2.0, by2 + (btn_h - line_w) / 2.0,
        icon_len, line_w, icon_color);

    // Theme toggle button (below zoom pill)
    let toggle_y = by + pill_h + gap * 2.0;
    // Border
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        bx - bw, toggle_y - bw, btn_w + bw * 2.0, btn_h + bw * 2.0,
        btn_w + bw * 2.0, btn_h + bw * 2.0, radius + bw, btn_border);
    // Background
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        bx, toggle_y, btn_w, btn_h, btn_w, btn_h, radius, btn_bg);
    // Sun/Moon icon: circle in center
    let icon_r = 5.0 * s * z;
    let cx = bx + btn_w / 2.0;
    let cy = toggle_y + btn_h / 2.0;
    if is_dark {
        // Moon: crescent (circle + smaller dark circle offset to the right)
        push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
            cx - icon_r, cy - icon_r, icon_r * 2.0, icon_r * 2.0,
            icon_r * 2.0, icon_r * 2.0, icon_r, icon_color);
        // Dark cutout circle offset
        push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
            cx - icon_r * 0.3, cy - icon_r * 1.0, icon_r * 1.6, icon_r * 1.6,
            icon_r * 1.6, icon_r * 1.6, icon_r * 0.8, btn_bg);
    } else {
        // Sun: circle + rays (small lines)
        push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
            cx - icon_r * 0.7, cy - icon_r * 0.7, icon_r * 1.4, icon_r * 1.4,
            icon_r * 1.4, icon_r * 1.4, icon_r * 0.7, icon_color);
        // 4 rays
        let ray_len = 3.0 * s * z;
        let ray_w = 1.5 * s * z;
        let offset = icon_r + 1.5 * s * z;
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices, cx - ray_w * 0.5, cy - offset - ray_len, ray_w, ray_len, icon_color); // top
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices, cx - ray_w * 0.5, cy + offset, ray_w, ray_len, icon_color); // bottom
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices, cx - offset - ray_len, cy - ray_w * 0.5, ray_len, ray_w, icon_color); // left
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices, cx + offset, cy - ray_w * 0.5, ray_len, ray_w, icon_color); // right
    }

    // Zoom percentage label (above the pill) — fixed screen size via `z`
    let zoom_pct = format!("{}%", (zoom * 100.0) as u32);
    let char_w = atlas.cell_width * z;
    let char_h = atlas.cell_height * z;
    let label_w = zoom_pct.len() as f32 * char_w;
    let label_x = bx + (btn_w - label_w) / 2.0;
    let label_y = by - char_h - 6.0 * s * z;
    let label_color = canvas_theme.label.to_array();
    let mut lx = label_x;
    for c in zoom_pct.chars() {
        if c != ' ' {
            let glyph = atlas.get_or_rasterize_ui(c, gpu_queue);
            if glyph.width > 0.0 && glyph.height > 0.0 {
                let gw = glyph.width * z;
                let gh = glyph.height * z;
                let gx = lx + glyph.bearing_x * z;
                let gy = label_y + (char_h - glyph.bearing_y * z);
                let base = batch.fg_verts.len() as u32;
                batch.fg_verts.extend_from_slice(&[
                    TextVertex { position: [gx, gy], tex_coords: [glyph.tex_x, glyph.tex_y], color: label_color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx + gw, gy], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y], color: label_color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx + gw, gy + gh], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y + glyph.tex_h], color: label_color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx, gy + gh], tex_coords: [glyph.tex_x, glyph.tex_y + glyph.tex_h], color: label_color, bg_color: [0.0; 4] },
                ]);
                batch.fg_indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
            }
        }
        lx += char_w;
    }

    batch
}
