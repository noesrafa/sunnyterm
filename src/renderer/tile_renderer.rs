use crate::app::Selection;
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
    selection: Option<&Selection>,
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
    let input_gap = 6.0 * s;
    let max_input_lines: usize = 5;

    // Base input bar height (1 line) — output area is always calculated from this
    let base_input_bar_h = input_padding * 2.0 + cell_h;

    // Actual input bar height grows upward based on content
    let line_count = pane.input_line_count();
    let visible_lines = line_count.min(max_input_lines).max(1);
    let input_bar_h = input_padding * 2.0 + visible_lines as f32 * cell_h;

    let content_y = ty + bar_h;
    // Output area stays fixed (based on single-line input)
    let output_area_h = th - base_input_bar_h - input_gap;
    // Input bar grows upward from the bottom of the tile
    let input_bar_y = ty + bar_h + th - input_bar_h;

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

    // Resize handle — diagonal grip lines
    {
        let grip_color = [border_color[0], border_color[1], border_color[2],
            (border_color[3] * 1.8).min(1.0)];
        let line_w = 1.5 * s;
        let margin = 4.0 * s;
        let corner_x = tx + tw - margin;
        let corner_y = ty + total_h - margin;
        // 3 diagonal lines from bottom-right corner
        for i in 0..3 {
            let offset = (i as f32 + 1.0) * 5.0 * s;
            let len_steps = 4i32;
            for step in 0..=len_steps {
                let t = step as f32 / len_steps as f32;
                let px = corner_x - offset * t;
                let py = corner_y - offset * (1.0 - t);
                push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                    px - line_w * 0.5, py - line_w * 0.5, line_w, line_w, grip_color);
            }
        }
    }

    // Active indicator dot (blue with lighter border, left of title)
    if is_focused {
        let dot_radius = 3.5 * s;
        let border_w = 1.2 * s;
        let dot_cx = tx + 14.0 * s;
        let title_y = ty + (bar_h - cell_h) / 2.0;
        let dot_cy = title_y + cell_h * 0.65;
        let dot_color = [0.25, 0.52, 1.0, 1.0];
        let border_color_dot = [0.45, 0.7, 1.0, 1.0];
        let steps = 10i32;
        // Border circle (slightly larger)
        let outer_r = dot_radius + border_w;
        for iy in -steps..=steps {
            let fy = iy as f32 / steps as f32;
            let half_w = (1.0 - fy * fy).sqrt() * outer_r;
            let py = dot_cy + fy * outer_r;
            let row_h = outer_r / steps as f32;
            push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                dot_cx - half_w, py - row_h / 2.0,
                half_w * 2.0, row_h, border_color_dot);
        }
        // Inner fill
        for iy in -steps..=steps {
            let fy = iy as f32 / steps as f32;
            let half_w = (1.0 - fy * fy).sqrt() * dot_radius;
            let py = dot_cy + fy * dot_radius;
            let row_h = dot_radius / steps as f32;
            push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                dot_cx - half_w, py - row_h / 2.0,
                half_w * 2.0, row_h, dot_color);
        }
    }

    // Title text
    render_title_text(&mut batch, tile, atlas, theme, canvas_theme, gpu_queue,
        is_renaming, rename_buffer, is_focused, ty, bar_h, tx, tw, s);

    // ── Close button (X) in top-right of title bar ──
    {
        let close_size = 28.0 * s;
        let close_margin = 8.0 * s;
        let x_size = 8.0 * s;
        let x_thick = 1.5 * s;
        let close_cx = tx + tw - close_size / 2.0 - close_margin;
        let close_cy = ty + (bar_h - cell_h) / 2.0 + cell_h * 0.75;
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

    // Build selection highlight
    let tile_sel = selection.filter(|s| s.tile_id == tile.id);
    let sel_color = if theme.background.to_array()[0] > 0.5 {
        [0.0, 0.4, 0.8, 0.3] // blue highlight for light theme
    } else {
        [0.3, 0.5, 0.9, 0.4] // brighter blue for dark theme
    };
    pane.text_renderer.build_selection(tile_sel, &pane.grid, atlas, padding, sel_color);

    let ox = tx;
    let is_alternate = pane.grid.alternate_screen;
    let full_grid = is_alternate || pane.passthrough;

    if full_grid {
        // ── Full grid mode (alternate screen or passthrough): no input bar ──
        let bg_base = batch.bg_verts.len() as u32;
        for v in &pane.text_renderer.bg_vertices {
            batch.bg_verts.push(TextVertex {
                position: [v.position[0] + ox, v.position[1] + content_y], ..*v
            });
        }
        for idx in &pane.text_renderer.bg_indices { batch.bg_indices.push(idx + bg_base); }

        // Selection highlight (between bg and fg)
        let sel_base = batch.bg_verts.len() as u32;
        for v in &pane.text_renderer.sel_vertices {
            batch.bg_verts.push(TextVertex {
                position: [v.position[0] + ox, v.position[1] + content_y], ..*v
            });
        }
        for idx in &pane.text_renderer.sel_indices { batch.bg_indices.push(idx + sel_base); }

        let fg_base = batch.fg_verts.len() as u32;
        for v in &pane.text_renderer.fg_vertices {
            batch.fg_verts.push(TextVertex {
                position: [v.position[0] + ox, v.position[1] + content_y], ..*v
            });
        }
        for idx in &pane.text_renderer.fg_indices { batch.fg_indices.push(idx + fg_base); }

        // Cursor only in alternate screen mode (not passthrough)
        if is_focused && is_alternate {
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

    // Selection highlight (between bg and fg, clipped to output area)
    for quad in pane.text_renderer.sel_vertices.chunks_exact(4) {
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
    // Background to cover output text when input grows upward (inset to preserve rounded corners)
    if visible_lines > 1 {
        let extra_h = (visible_lines - 1) as f32 * cell_h;
        let input_bg_color = theme.background.to_array();
        let inset = corner_radius;
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            tx + inset, input_bar_y - input_gap,
            tw - inset * 2.0, extra_h + input_gap, input_bg_color);
        // Left and right edges (narrower, inside the rounded corners)
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            tx, input_bar_y,
            inset, extra_h, input_bg_color);
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            tx + tw - inset, input_bar_y,
            inset, extra_h, input_bg_color);
    }

    // Separator line between output and input
    let sep_color = canvas_theme.tile_border.to_array();
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        tx + padding, input_bar_y - input_gap / 2.0,
        tw - padding * 2.0, bw * 2.0, sep_color);

    // Render multiline input buffer text
    let input_text_x = tx + padding;
    let fg_color = theme.foreground.to_array();
    let max_x = tx + tw - padding;
    let input_scroll = pane.input_scroll.min(line_count.saturating_sub(visible_lines));

    let lines = pane.input_lines();
    for (vis_idx, line_idx) in (input_scroll..lines.len().min(input_scroll + max_input_lines)).enumerate() {
        let line = lines[line_idx];
        let line_y = input_bar_y + vis_idx as f32 * cell_h;
        let mut char_x = input_text_x;
        for c in line.chars() {
            if char_x + cell_w > max_x { break; }
            if c != ' ' {
                let glyph = atlas.get_or_rasterize(c, false, false, gpu_queue);
                if glyph.width > 0.0 && glyph.height > 0.0 {
                    let gx = char_x + glyph.bearing_x;
                    let gy = line_y + (cell_h - glyph.bearing_y);
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
    }

    // Cursor in input bar (multiline)
    if is_focused {
        let (cursor_row, cursor_col) = pane.input_cursor_pos();
        // Only draw if cursor is in the visible range
        if cursor_row >= input_scroll && cursor_row < input_scroll + max_input_lines {
            let vis_row = cursor_row - input_scroll;
            let cursor_x = input_text_x + cursor_col as f32 * cell_w;
            let cursor_line_y = input_bar_y + vis_row as f32 * cell_h;
            let cursor_color = theme.cursor.to_array();

            let beam_width = (cell_w * 0.55).max(5.0);
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
                let cy = cursor_line_y + cy_offset;
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
    let mut title_x = tx + if is_focused { 26.0 } else { 10.0 } * s;
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
