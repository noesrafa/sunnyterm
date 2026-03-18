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
    let cell_h = atlas.cell_height;
    let cell_w = atlas.cell_width;

    let border_color = canvas_theme.tile_border.to_array();
    let bw = s;
    let bw2 = bw * 2.0;
    let br = corner_radius + bw;

    // ── Layout ──
    let input_padding = 8.0 * s;
    let input_bar_h = input_padding * 2.0 + cell_h;
    let input_gap = 6.0 * s;

    let content_y = ty + bar_h;
    let output_area_h = th - input_bar_h - input_gap;
    let input_bar_y = ty + bar_h + output_area_h + input_gap;

    // 1) Border
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx - bw, ty - bw, tw + bw2, total_h + bw2, tw + bw2, total_h + bw2, br, border_color);

    // 2) Tile background
    let tile_bg = theme.background.to_array();
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx, ty, tw, total_h, tw, total_h, corner_radius, tile_bg);

    // 3) Title bar
    let bar_color = canvas_theme.tile_bar.to_array();
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx, ty, tw, bar_h, tw, total_h, corner_radius, bar_color);

    // Resize handle
    let handle_size = 8.0 * s;
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        tx + tw - handle_size - 2.0 * s, ty + total_h - 3.0 * s,
        handle_size, bw, border_color);
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        tx + tw - 3.0 * s, ty + total_h - handle_size - 2.0 * s,
        bw, handle_size, border_color);

    // Title text
    render_title_text(&mut batch, tile, atlas, theme, canvas_theme, gpu_queue,
        is_renaming, rename_buffer, is_focused, ty, bar_h, tx, tw, s);

    // ── Close button (X) in top-right of title bar ──
    {
        let close_size = 28.0 * s;
        let close_margin = 2.0 * s;
        let x_size = 8.0 * s;
        let x_thick = 1.5 * s;
        let close_cx = tx + tw - close_size / 2.0 - close_margin;
        let close_cy = ty + bar_h / 2.0;
        let x_color = canvas_theme.title_unfocused.to_array();

        // Draw X as two quads rotated 45deg (approximated with small rects)
        // Line from top-left to bottom-right
        let half = x_size / 2.0;
        let steps = 6i32;
        for i in -steps..=steps {
            let frac = i as f32 / steps as f32;
            let px = close_cx + frac * half - x_thick / 2.0;
            let py = close_cy + frac * half - x_thick / 2.0;
            push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                px, py, x_thick, x_thick, x_color);
        }
        // Line from top-right to bottom-left
        for i in -steps..=steps {
            let frac = i as f32 / steps as f32;
            let px = close_cx - frac * half - x_thick / 2.0;
            let py = close_cy + frac * half - x_thick / 2.0;
            push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                px, py, x_thick, x_thick, x_color);
        }
    }

    // ── Render pane content ──
    pane.cursor_renderer.visible = is_focused;
    pane.cursor_renderer.update();

    pane.text_renderer.build_vertices(
        &pane.grid, atlas, theme, padding, gpu_queue,
    );

    let ox = tx;
    let is_alternate = pane.grid.alternate_screen;

    if is_alternate {
        // ── Alternate screen mode: full grid, no input bar ──
        let bg_base = batch.bg_verts.len() as u32;
        for v in &pane.text_renderer.bg_vertices {
            batch.bg_verts.push(TextVertex {
                position: [v.position[0] + ox, v.position[1] + content_y], ..*v
            });
        }
        for idx in &pane.text_renderer.bg_indices { batch.bg_indices.push(idx + bg_base); }

        let fg_base = batch.fg_verts.len() as u32;
        for v in &pane.text_renderer.fg_vertices {
            batch.fg_verts.push(TextVertex {
                position: [v.position[0] + ox, v.position[1] + content_y], ..*v
            });
        }
        for idx in &pane.text_renderer.fg_indices { batch.fg_indices.push(idx + fg_base); }

        // Cursor in alternate mode
        if is_focused {
            let (cverts, cidxs) = pane.cursor_renderer.build_vertices(
                pane.grid.cursor_row, pane.grid.cursor_col,
                cell_w, cell_h, padding, cursor_style, theme,
            );
            let cur_base = batch.bg_verts.len() as u32;
            for v in &cverts {
                batch.bg_verts.push(TextVertex {
                    position: [v.position[0] + ox, v.position[1] + content_y], ..*v
                });
            }
            for idx in &cidxs { batch.bg_indices.push(idx + cur_base); }
        }

        return batch;
    }

    // ── Chat mode: output area + input bar ──
    // Bottom-anchor output
    let grid_content_h = padding + pane.grid.rows as f32 * cell_h;
    let output_scroll = if grid_content_h > output_area_h {
        grid_content_h - output_area_h
    } else {
        0.0
    };
    let output_oy = content_y - output_scroll;

    let clip_top = content_y;
    let clip_bottom = content_y + output_area_h;

    for quad in pane.text_renderer.bg_vertices.chunks_exact(4) {
        let qy = quad[0].position[1] + output_oy;
        if qy + cell_h <= clip_top || qy >= clip_bottom { continue; }
        let base = batch.bg_verts.len() as u32;
        for v in quad {
            batch.bg_verts.push(TextVertex {
                position: [v.position[0] + ox, v.position[1] + output_oy], ..*v
            });
        }
        batch.bg_indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
    }

    for quad in pane.text_renderer.fg_vertices.chunks_exact(4) {
        let qy = quad[0].position[1] + output_oy;
        if qy + cell_h <= clip_top || qy >= clip_bottom { continue; }
        let base = batch.fg_verts.len() as u32;
        for v in quad {
            batch.fg_verts.push(TextVertex {
                position: [v.position[0] + ox, v.position[1] + output_oy], ..*v
            });
        }
        batch.fg_indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
    }

    // ── Input bar ──
    // Separator line between output and input
    let sep_color = canvas_theme.tile_border.to_array();
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        tx + padding, input_bar_y - input_gap / 2.0,
        tw - padding * 2.0, bw * 2.0, sep_color);

    // Render input buffer text
    let input_text_x = tx + padding;
    let input_text_y = input_bar_y;
    let fg_color = theme.foreground.to_array();
    let max_x = tx + tw - padding;

    let mut char_x = input_text_x;
    for c in pane.input_buffer.chars() {
        if char_x + cell_w > max_x { break; }
        if c != ' ' {
            let glyph = atlas.get_or_rasterize(c, false, false, gpu_queue);
            if glyph.width > 0.0 && glyph.height > 0.0 {
                let gx = char_x + glyph.bearing_x;
                let gy = input_text_y + (cell_h - glyph.bearing_y);
                let fg_base = batch.fg_verts.len() as u32;
                batch.fg_verts.extend_from_slice(&[
                    TextVertex { position: [gx, gy], tex_coords: [glyph.tex_x, glyph.tex_y], color: fg_color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx + glyph.width, gy], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y], color: fg_color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx + glyph.width, gy + glyph.height], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y + glyph.tex_h], color: fg_color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx, gy + glyph.height], tex_coords: [glyph.tex_x, glyph.tex_y + glyph.tex_h], color: fg_color, bg_color: [0.0; 4] },
                ]);
                batch.fg_indices.extend_from_slice(&[fg_base, fg_base+1, fg_base+2, fg_base, fg_base+2, fg_base+3]);
            }
        }
        char_x += cell_w;
    }

    // Cursor in input bar
    if is_focused {
        let cursor_col = pane.input_cursor_col();
        let cursor_x = input_text_x + cursor_col as f32 * cell_w;
        let cursor_color = theme.cursor.to_array();

        let beam_width = (cell_w * 0.4).max(4.0);
        let reduced_h = cell_h * 0.75;
        let (cw, ch) = match cursor_style {
            "beam" => (beam_width, reduced_h),
            "underline" => (cell_w, beam_width),
            _ => (cell_w, reduced_h),
        };
        let cy_offset = match cursor_style {
            "underline" => cell_h - beam_width,
            _ => cell_h - ch,
        };

        if pane.cursor_renderer.visible && (!pane.cursor_renderer.blink || pane.cursor_renderer.blink_on()) {
            let cx = cursor_x;
            let cy = input_text_y + cy_offset;
            let cur_base = batch.bg_verts.len() as u32;
            batch.bg_verts.extend_from_slice(&[
                TextVertex { position: [cx, cy], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: cursor_color },
                TextVertex { position: [cx + cw, cy], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: cursor_color },
                TextVertex { position: [cx + cw, cy + ch], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: cursor_color },
                TextVertex { position: [cx, cy + ch], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: cursor_color },
            ]);
            batch.bg_indices.extend_from_slice(&[cur_base, cur_base+1, cur_base+2, cur_base, cur_base+2, cur_base+3]);
        }
    }

    batch
}

fn render_title_text(
    batch: &mut DrawBatch,
    tile: &Tile,
    atlas: &mut GlyphAtlas,
    theme: &Theme,
    canvas_theme: &CanvasTheme,
    gpu_queue: &wgpu::Queue,
    is_renaming: bool,
    rename_buffer: &str,
    is_focused: bool,
    ty: f32,
    bar_h: f32,
    tx: f32,
    tw: f32,
    s: f32,
) {
    let cell_h = atlas.cell_height;
    let cell_w = atlas.cell_width;

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
    let title_y = ty + (bar_h - cell_h) / 2.0;
    let mut title_x = tx + 10.0 * s;
    for c in display_name.chars() {
        if title_x + cell_w > tx + tw - 10.0 * s {
            break;
        }
        if c != ' ' {
            let glyph = atlas.get_or_rasterize(c, false, false, gpu_queue);
            if glyph.width > 0.0 && glyph.height > 0.0 {
                let gx = title_x + glyph.bearing_x;
                let gy = title_y + (cell_h - glyph.bearing_y);
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
        title_x += cell_w;
    }
}
