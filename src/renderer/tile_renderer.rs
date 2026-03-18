use crate::pane::Pane;
use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::draw_helpers::{push_quad, push_rounded_quad, DrawBatch};
use crate::renderer::text::TextVertex;
use crate::ui::canvas::Tile;
use crate::ui::canvas_theme::CanvasTheme;
use crate::ui::theme::Theme;

pub fn build_tile_batch(
    tile: &Tile,
    pane: &mut Pane,
    atlas: &mut GlyphAtlas,
    theme: &Theme,
    canvas_theme: &CanvasTheme,
    gpu_queue: &wgpu::Queue,
    scale_factor: f32,
    bar_h: f32,
    padding: f32,
    corner_radius: f32,
    is_focused: bool,
    is_renaming: bool,
    rename_buffer: &str,
    cursor_style: &str,
) -> DrawBatch {
    let mut batch = DrawBatch::new();

    let tx = tile.x;
    let ty = tile.y;
    let tw = tile.w;
    let th = tile.h;
    let total_h = th + bar_h;
    let s = scale_factor;

    let border_color = canvas_theme.tile_border.to_array();
    let bw = s;
    let bw2 = bw * 2.0;
    let br = corner_radius + bw;

    // 1) Border (larger rounded rect, drawn first = behind)
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx - bw, ty - bw, tw + bw2, total_h + bw2, tw + bw2, total_h + bw2, br, border_color);

    // 2) Tile background (rounded)
    let tile_bg = theme.background.to_array();
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx, ty, tw, total_h, tw, total_h, corner_radius, tile_bg);

    // 3) Title bar
    let bar_color = canvas_theme.tile_bar.to_array();
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx, ty, tw, bar_h, tw, total_h, corner_radius, bar_color);

    let content_y = ty + bar_h;
    // Separator line
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices, tx + 1.0, content_y, tw - 2.0, bw * 0.5, border_color);

    // Resize handle indicator (small triangle-ish in bottom-right corner)
    let handle_size = 8.0 * s;
    let handle_color = border_color;
    // Two small lines to suggest a grip
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        tx + tw - handle_size - 2.0 * s, ty + total_h - 3.0 * s,
        handle_size, bw, handle_color);
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        tx + tw - 3.0 * s, ty + total_h - handle_size - 2.0 * s,
        bw, handle_size, handle_color);

    // Title text
    let display_name = if is_renaming {
        format!("{}|", rename_buffer)
    } else {
        tile.name.clone()
    };
    let title_color = if is_renaming {
        theme.foreground.to_array()
    } else if is_focused {
        canvas_theme.title_focused.to_array()
    } else {
        canvas_theme.title_unfocused.to_array()
    };
    let title_y = ty + (bar_h - atlas.cell_height) / 2.0;
    let mut title_x = tx + 10.0 * s;
    for c in display_name.chars() {
        if title_x + atlas.cell_width > tx + tw - 10.0 * s {
            break;
        }
        if c != ' ' {
            let glyph = atlas.get_or_rasterize(c, false, false, gpu_queue);
            if glyph.width > 0.0 && glyph.height > 0.0 {
                let gx = title_x + glyph.bearing_x;
                let gy = title_y + (atlas.cell_height - glyph.bearing_y);
                let fg_base = batch.fg_verts.len() as u32;
                batch.fg_verts.extend_from_slice(&[
                    TextVertex { position: [gx, gy], tex_coords: [glyph.tex_x, glyph.tex_y], color: title_color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx + glyph.width, gy], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y], color: title_color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx + glyph.width, gy + glyph.height], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y + glyph.tex_h], color: title_color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx, gy + glyph.height], tex_coords: [glyph.tex_x, glyph.tex_y + glyph.tex_h], color: title_color, bg_color: [0.0; 4] },
                ]);
                batch.fg_indices.extend_from_slice(&[fg_base, fg_base+1, fg_base+2, fg_base, fg_base+2, fg_base+3]);
            }
        }
        title_x += atlas.cell_width;
    }

    // Pane text
    pane.cursor_renderer.visible = is_focused;
    pane.cursor_renderer.update();
    pane.text_renderer.build_vertices(
        &pane.grid, atlas, theme, padding, gpu_queue,
    );

    let ox = tx;
    let oy = content_y;

    let bg_base = batch.bg_verts.len() as u32;
    for v in &pane.text_renderer.bg_vertices {
        batch.bg_verts.push(TextVertex { position: [v.position[0] + ox, v.position[1] + oy], ..*v });
    }
    for idx in &pane.text_renderer.bg_indices { batch.bg_indices.push(idx + bg_base); }

    let fg_base = batch.fg_verts.len() as u32;
    for v in &pane.text_renderer.fg_vertices {
        batch.fg_verts.push(TextVertex { position: [v.position[0] + ox, v.position[1] + oy], ..*v });
    }
    for idx in &pane.text_renderer.fg_indices { batch.fg_indices.push(idx + fg_base); }

    if is_focused {
        let (cverts, cidxs) = pane.cursor_renderer.build_vertices(
            pane.grid.cursor_row, pane.grid.cursor_col,
            atlas.cell_width, atlas.cell_height,
            padding, cursor_style, theme,
        );
        let cur_base = batch.bg_verts.len() as u32;
        for v in &cverts { batch.bg_verts.push(TextVertex { position: [v.position[0] + ox, v.position[1] + oy], ..*v }); }
        for idx in &cidxs { batch.bg_indices.push(idx + cur_base); }
    }

    batch
}
