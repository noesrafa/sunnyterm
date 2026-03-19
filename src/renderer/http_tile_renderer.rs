use crate::http_pane::{HttpField, HttpPane, ResponseView, TreeValueKind};
use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::draw_helpers::{push_quad, push_rounded_quad, DrawBatch};
use crate::renderer::text::TextVertex;
use crate::ui::canvas::Tile;
use crate::ui::canvas_theme::CanvasTheme;
use crate::ui::theme::Theme;

pub fn build_http_tile_batch(
    tile: &Tile,
    http: &mut HttpPane,
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
    let is_light = theme.background.to_array()[0] > 0.5;

    let border_color = canvas_theme.tile_border.to_array();
    let bw = s;
    let bw2 = bw * 2.0;
    let br = corner_radius + bw;
    let tile_bg = theme.background.to_array();
    let fg_color = theme.foreground.to_array();
    let muted = [fg_color[0] * 0.45, fg_color[1] * 0.45, fg_color[2] * 0.45, 0.7];
    let placeholder = [fg_color[0] * 0.3, fg_color[1] * 0.3, fg_color[2] * 0.3, 0.5];

    // Border-only style: no fill backgrounds, just border outlines
    let field_border = if is_light {
        [0.0, 0.0, 0.0, 0.06]
    } else {
        [1.0, 1.0, 1.0, 0.06]
    };
    let field_border_active = [0.4, 0.6, 1.0, 0.35];


    let pad = padding;
    let gap = 8.0 * s;
    let field_r = 6.0 * s;
    let field_pad = 8.0 * s;
    let field_h = cell_h + field_pad * 2.0;

    // ── Tile chrome (border, bg, title bar, close button) ──
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx - bw, ty - bw, tw + bw2, total_h + bw2, tw + bw2, total_h + bw2, br, border_color);
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx, ty, tw, total_h, tw, total_h, corner_radius, tile_bg);
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

    // Active dot (orange)
    if is_focused {
        let dot_r = 3.5 * s;
        let dot_cx = tx + 14.0 * s;
        let title_y = ty + (bar_h - cell_h) / 2.0;
        let dot_cy = title_y + cell_h * 0.65;
        let dot_color = [0.25, 0.52, 1.0, 1.0];
        let outer_r = dot_r + 1.2 * s;
        let border_c = [0.45, 0.7, 1.0, 1.0];
        let steps = 10i32;
        for iy in -steps..=steps {
            let fy = iy as f32 / steps as f32;
            let hw = (1.0 - fy * fy).sqrt();
            let py = dot_cy + fy * outer_r;
            let rh = outer_r / steps as f32;
            push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                dot_cx - hw * outer_r, py - rh / 2.0, hw * outer_r * 2.0, rh, border_c);
        }
        for iy in -steps..=steps {
            let fy = iy as f32 / steps as f32;
            let hw = (1.0 - fy * fy).sqrt();
            let py = dot_cy + fy * dot_r;
            let rh = dot_r / steps as f32;
            push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                dot_cx - hw * dot_r, py - rh / 2.0, hw * dot_r * 2.0, rh, dot_color);
        }
    }

    // Title + close button
    render_title_text(&mut batch, tile, atlas, theme, canvas_theme, gpu_queue,
        is_renaming, rename_buffer, is_focused, ty, bar_h, tx, tw, s);
    render_close_button(&mut batch, canvas_theme, tx, ty, tw, bar_h, cell_h, s);

    // ── Content ──
    let content_x = tx + pad;
    let content_w = tw - pad * 2.0;
    let mut cy = ty + bar_h + pad;
    let max_chars = ((content_w - field_pad * 2.0) / cell_w) as usize;

    // Helper: draw a rounded border-only rect (outer rounded quad as border, inner as tile bg)
    macro_rules! draw_field_border {
        ($bx:expr, $by:expr, $bw_:expr, $bh:expr, $focused:expr) => {
            let bc = if $focused { field_border_active } else { field_border };
            let bt = bw; // border thickness
            // Outer (border color)
            push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
                $bx, $by, $bw_, $bh, $bw_, $bh, field_r, bc);
            // Inner (tile bg to punch out the fill, leaving only the border)
            push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
                $bx + bt, $by + bt, $bw_ - bt * 2.0, $bh - bt * 2.0,
                $bw_ - bt * 2.0, $bh - bt * 2.0, (field_r - bt).max(0.0), tile_bg);
        };
    }

    // ── Method + URL + cURL + Enviar row ──
    let method_w = (http.method.as_str().len() as f32 + 2.0) * cell_w;
    let send_text = if http.loading { "..." } else { "Enviar" };
    let send_btn_w = (send_text.len() as f32 + 2.0) * cell_w;
    let curl_text = "cURL";
    let curl_btn_w = (curl_text.len() as f32 + 1.5) * cell_w;
    let url_x = content_x + method_w + gap;
    let url_w = content_w - method_w - gap - gap * 0.5 - curl_btn_w - gap * 0.5 - send_btn_w;
    let curl_btn_x = url_x + url_w + gap * 0.5;
    let send_x = curl_btn_x + curl_btn_w + gap * 0.5;

    // Method
    let method_focused = http.focus_field == HttpField::Method && is_focused;
    draw_field_border!(content_x, cy, method_w, field_h, method_focused);
    render_text_line(&mut batch, atlas, gpu_queue,
        content_x + field_pad, cy + field_pad, http.method.as_str(), http.method.color(), cell_w, cell_h);

    // URL
    let url_focused = http.focus_field == HttpField::Url && is_focused;
    draw_field_border!(url_x, cy, url_w, field_h, url_focused);
    let url_text_x = url_x + field_pad;
    let url_text_y = cy + field_pad;
    let max_url_chars = ((url_w - field_pad * 2.0) / cell_w) as usize;
    if http.url.is_empty() {
        render_text_line(&mut batch, atlas, gpu_queue,
            url_text_x, url_text_y, "Enter URL or paste curl...", placeholder, cell_w, cell_h);
    } else {
        let cursor_chars = http.url[..http.url_cursor].chars().count();
        let scroll = if cursor_chars > max_url_chars.saturating_sub(2) {
            cursor_chars.saturating_sub(max_url_chars.saturating_sub(2))
        } else { 0 };
        let display: String = http.url.chars().skip(scroll).take(max_url_chars).collect();
        render_text_line(&mut batch, atlas, gpu_queue,
            url_text_x, url_text_y, &display, fg_color, cell_w, cell_h);
        if url_focused {
            let vis_cursor = cursor_chars.saturating_sub(scroll);
            let cx = url_text_x + vis_cursor as f32 * cell_w;
            if http.cursor_renderer.visible && (!http.cursor_renderer.blink || http.cursor_renderer.blink_on()) {
                push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                    cx, url_text_y, 2.0 * s, cell_h, theme.cursor.to_array());
            }
        }
    }

    // cURL button
    draw_field_border!(curl_btn_x, cy, curl_btn_w, field_h, false);
    render_text_line(&mut batch, atlas, gpu_queue,
        curl_btn_x + (curl_btn_w - curl_text.len() as f32 * cell_w) / 2.0,
        cy + field_pad, curl_text, muted, cell_w, cell_h);

    // Enviar button (fully rounded pill)
    let send_color = if http.loading { [0.25, 0.25, 0.3, 1.0] } else { [0.15, 0.3, 0.7, 1.0] };
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        send_x, cy, send_btn_w, field_h, send_btn_w, field_h, field_h / 2.0, send_color);
    render_text_line(&mut batch, atlas, gpu_queue,
        send_x + (send_btn_w - send_text.len() as f32 * cell_w) / 2.0,
        cy + field_pad, send_text, [1.0, 1.0, 1.0, 0.95], cell_w, cell_h);

    cy += field_h + gap;

    // ── Headers ──
    let headers_focused = http.focus_field == HttpField::Headers && is_focused;
    if !http.headers.is_empty() || headers_focused {
        let header_label = format!("Headers ({})", http.headers.len());
        render_text_line(&mut batch, atlas, gpu_queue,
            content_x + 2.0 * s, cy, &header_label, muted, cell_w, cell_h);
        if headers_focused {
            // Hints: Enter=add, Backspace on empty=delete
            let hint = "Enter:add  Bksp:del";
            let hint_w = hint.len() as f32 * cell_w;
            render_text_line(&mut batch, atlas, gpu_queue,
                content_x + content_w - hint_w, cy, hint, placeholder, cell_w, cell_h);
        }
        cy += cell_h + 4.0 * s;

        // Show all headers
        let row_h = cell_h + 2.0 * s;
        let header_count = http.headers.len();

        for i in 0..header_count {
            let (key, val) = &http.headers[i];
            let is_selected = headers_focused && http.header_edit_index == i;

            // Selection indicator
            if is_selected {
                let sel_color = field_border_active;
                push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                    content_x, cy + 1.0 * s, 2.0 * s, cell_h, sel_color);
            }

            let text_x = content_x + if is_selected { 6.0 * s } else { 4.0 * s };
            let key_display = format!("{}:", key);
            let key_chars = key_display.chars().count();
            // Key: color differently if editing key
            let key_color = if is_selected && http.header_edit_field == 0 { fg_color } else { muted };
            render_text_line(&mut batch, atlas, gpu_queue,
                text_x, cy + 1.0 * s, &key_display, key_color, cell_w, cell_h);

            // Space between key: and value
            let val_x = text_x + (key_chars as f32 + 1.0) * cell_w;
            let max_val = max_chars.saturating_sub(key_chars + 1);
            let val_display: String = val.chars().take(max_val).collect();
            let val_color = if is_selected && http.header_edit_field == 1 { fg_color } else {
                if is_selected { fg_color } else { fg_color }
            };
            render_text_line(&mut batch, atlas, gpu_queue,
                val_x, cy + 1.0 * s, &val_display, val_color, cell_w, cell_h);

            // Cursor within header
            if is_selected && is_focused {
                let cursor_col = http.header_cursor_col();
                let cursor_x = if http.header_edit_field == 0 {
                    // Cursor in key (before the ":")
                    text_x + cursor_col as f32 * cell_w
                } else {
                    // Cursor in value (after "key: ")
                    val_x + cursor_col as f32 * cell_w
                };
                if http.cursor_renderer.visible && (!http.cursor_renderer.blink || http.cursor_renderer.blink_on()) {
                    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                        cursor_x, cy + 1.0 * s, 2.0 * s, cell_h, theme.cursor.to_array());
                }
            }

            cy += row_h;
        }

        cy += gap * 0.5;
    }

    // ── Body ──
    let body_focused = http.focus_field == HttpField::Body && is_focused;
    let has_body = !http.body.is_empty() || body_focused;
    if has_body {
        render_text_line(&mut batch, atlas, gpu_queue,
            content_x + 2.0 * s, cy, "Body", muted, cell_w, cell_h);
        cy += cell_h + 4.0 * s;

        let max_body_lines = 10;
        let body_lines: Vec<&str> = http.body.lines().collect();
        let total_body_lines = body_lines.len().max(1);
        let visible = total_body_lines.min(max_body_lines);
        let body_h = visible as f32 * cell_h + field_pad * 2.0;

        draw_field_border!(content_x, cy, content_w, body_h, body_focused);

        let body_text_x = content_x + field_pad;
        let body_max_chars = ((content_w - field_pad * 2.0) / cell_w) as usize;

        // JSON colors for body
        let body_is_json = http.body.trim_start().starts_with('{')
            || http.body.trim_start().starts_with('[');
        let body_json_colors = if body_is_json {
            Some(JsonColors {
                key: if is_light { [0.55, 0.1, 0.55, 1.0] } else { [0.65, 0.55, 0.95, 1.0] },
                string: if is_light { [0.1, 0.5, 0.1, 1.0] } else { [0.55, 0.85, 0.55, 1.0] },
                number: if is_light { [0.7, 0.4, 0.0, 1.0] } else { [0.85, 0.65, 0.35, 1.0] },
                boolean: if is_light { [0.0, 0.4, 0.7, 1.0] } else { [0.45, 0.7, 0.95, 1.0] },
                null_color: if is_light { [0.5, 0.5, 0.5, 1.0] } else { [0.55, 0.55, 0.6, 1.0] },
                bracket: muted, punctuation: muted,
            })
        } else { None };

        // Scroll: keep cursor visible
        let cursor_row = if body_focused {
            let before = &http.body[..http.body_cursor];
            before.matches('\n').count()
        } else { 0 };
        let body_scroll = if total_body_lines <= max_body_lines {
            0
        } else if cursor_row >= max_body_lines {
            (cursor_row - max_body_lines + 1).min(total_body_lines - max_body_lines)
        } else {
            0
        };

        if http.body.is_empty() {
            render_text_line(&mut batch, atlas, gpu_queue,
                body_text_x, cy + field_pad, "Request body...", placeholder, cell_w, cell_h);
        } else {
            for (vis_i, line_i) in (body_scroll..total_body_lines.min(body_scroll + max_body_lines)).enumerate() {
                if line_i < body_lines.len() {
                    let display: String = body_lines[line_i].chars().take(body_max_chars).collect();
                    let line_y = cy + field_pad + vis_i as f32 * cell_h;
                    if let Some(ref colors) = body_json_colors {
                        render_json_line(&mut batch, atlas, gpu_queue,
                            body_text_x, line_y, &display, fg_color, colors, cell_w, cell_h);
                    } else {
                        render_text_line(&mut batch, atlas, gpu_queue,
                            body_text_x, line_y, &display, fg_color, cell_w, cell_h);
                    }
                }
            }

            // Scrollbar for body
            if total_body_lines > max_body_lines {
                render_scrollbar(&mut batch, content_x + content_w - 4.0 * s, cy,
                    body_h, total_body_lines, max_body_lines, body_scroll, s, is_light);
            }
        }

        // Cursor
        if body_focused {
            let before = &http.body[..http.body_cursor];
            let cr = before.matches('\n').count();
            let last_nl = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
            let cc = before[last_nl..].chars().count();
            let vis_row = cr.saturating_sub(body_scroll);
            if vis_row < max_body_lines {
                let bx = body_text_x + cc as f32 * cell_w;
                let by = cy + field_pad + vis_row as f32 * cell_h;
                if http.cursor_renderer.visible && (!http.cursor_renderer.blink || http.cursor_renderer.blink_on()) {
                    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                        bx, by, 2.0 * s, cell_h, theme.cursor.to_array());
                }
            }
        }

        cy += body_h + gap;
    }

    // ── Separator ──
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        content_x, cy, content_w, bw, field_border);
    cy += bw + gap;

    // ── Response ──
    let response_top = cy;
    let response_bottom = ty + total_h - pad;
    let resp_h = response_bottom - response_top;

    if let Some(status) = http.response_status {
        // Status + tabs row
        let status_text = format!("{}", status);
        let status_color = http.status_color();
        render_text_line(&mut batch, atlas, gpu_queue,
            content_x, cy, &status_text, status_color, cell_w, cell_h);

        let mut info_x = content_x + (status_text.len() as f32 + 1.0) * cell_w;
        render_text_line(&mut batch, atlas, gpu_queue, info_x, cy, "\u{00B7}", muted, cell_w, cell_h);
        info_x += 2.0 * cell_w;
        if let Some(ms) = http.response_time_ms {
            let time_str = if ms < 1000 { format!("{}ms", ms) } else { format!("{:.1}s", ms as f64 / 1000.0) };
            render_text_line(&mut batch, atlas, gpu_queue, info_x, cy, &time_str, muted, cell_w, cell_h);
            info_x += (time_str.len() as f32 + 1.0) * cell_w;
            render_text_line(&mut batch, atlas, gpu_queue, info_x, cy, "\u{00B7}", muted, cell_w, cell_h);
            info_x += 2.0 * cell_w;
        }
        let size_str = http.response_size_display();
        render_text_line(&mut batch, atlas, gpu_queue, info_x, cy, &size_str, muted, cell_w, cell_h);

        // Tabs + Copy on right: Copy  Raw  Tree
        let raw_active = http.response_view == ResponseView::Raw;
        let tree_active = http.response_view == ResponseView::Tree;
        let tab_copy = "Copy";
        let tab_raw = "Raw";
        let tab_tree = "Tree";
        let total_tabs_w = (tab_copy.len() + 2 + tab_raw.len() + 2 + tab_tree.len()) as f32 * cell_w;
        let tab_copy_x = content_x + content_w - total_tabs_w;
        let tab_raw_x = tab_copy_x + (tab_copy.len() as f32 + 2.0) * cell_w;
        let tab_tree_x = tab_raw_x + (tab_raw.len() as f32 + 2.0) * cell_w;

        // Copy button
        render_text_line(&mut batch, atlas, gpu_queue,
            tab_copy_x, cy, tab_copy, muted, cell_w, cell_h);
        render_text_line(&mut batch, atlas, gpu_queue,
            tab_raw_x, cy, tab_raw, if raw_active { fg_color } else { muted }, cell_w, cell_h);
        if raw_active {
            push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                tab_raw_x, cy + cell_h + 1.0 * s, tab_raw.len() as f32 * cell_w, 2.0 * s, field_border_active);
        }
        render_text_line(&mut batch, atlas, gpu_queue,
            tab_tree_x, cy, tab_tree, if tree_active { fg_color } else { muted }, cell_w, cell_h);
        if tree_active {
            push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                tab_tree_x, cy + cell_h + 1.0 * s, tab_tree.len() as f32 * cell_w, 2.0 * s, field_border_active);
        }

        cy += cell_h + gap * 0.5;

        // ── Search bar (Raw mode only) ──
        if http.search_active && raw_active {
            let search_h = field_h;
            draw_field_border!(content_x, cy, content_w, search_h,
                http.focus_field == HttpField::Search);

            // Search icon (magnifying glass as text)
            let search_icon_color = muted;
            render_text_line(&mut batch, atlas, gpu_queue,
                content_x + field_pad, cy + field_pad, "/", search_icon_color, cell_w, cell_h);

            let search_text_x = content_x + field_pad + 2.0 * cell_w;
            let search_max = ((content_w - field_pad * 2.0 - 2.0 * cell_w) / cell_w) as usize;

            if http.search_query.is_empty() {
                render_text_line(&mut batch, atlas, gpu_queue,
                    search_text_x, cy + field_pad, "Search response...", placeholder, cell_w, cell_h);
            } else {
                let display: String = http.search_query.chars().take(search_max).collect();
                render_text_line(&mut batch, atlas, gpu_queue,
                    search_text_x, cy + field_pad, &display, fg_color, cell_w, cell_h);
            }

            // Match count on right side
            if !http.search_query.is_empty() {
                let match_info = if http.search_matches.is_empty() {
                    "No matches".to_string()
                } else {
                    format!("{}/{}", http.search_current + 1, http.search_matches.len())
                };
                let info_w = match_info.len() as f32 * cell_w;
                let match_color = if http.search_matches.is_empty() {
                    [0.95, 0.4, 0.4, 0.8]
                } else {
                    muted
                };
                render_text_line(&mut batch, atlas, gpu_queue,
                    content_x + content_w - field_pad - info_w, cy + field_pad,
                    &match_info, match_color, cell_w, cell_h);
            }

            // Search cursor
            if http.focus_field == HttpField::Search && is_focused {
                let cursor_col = http.search_query[..http.search_cursor].chars().count();
                let scx = search_text_x + cursor_col as f32 * cell_w;
                if http.cursor_renderer.visible && (!http.cursor_renderer.blink || http.cursor_renderer.blink_on()) {
                    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                        scx, cy + field_pad, 2.0 * s, cell_h, theme.cursor.to_array());
                }
            }

            cy += search_h + gap * 0.5;
        }

        // Response body area
        let body_area_h = response_bottom - cy;
        if body_area_h > cell_h {
            draw_field_border!(content_x, cy, content_w, body_area_h, false);

            let resp_max = ((content_w - field_pad * 2.0) / cell_w) as usize;
            let available = ((body_area_h - field_pad) / cell_h).max(0.0) as usize;
            let ly = cy + field_pad * 0.5;

            if tree_active {
                // ── Tree view ──
                let tree_lines = http.build_tree_lines();
                let total = tree_lines.len();
                let start = http.tree_scroll.min(total.saturating_sub(1));
                let indent_w = 2.0 * cell_w;
                let arrow_color = muted;
                let jc = JsonColors {
                    key: if is_light { [0.55, 0.1, 0.55, 1.0] } else { [0.65, 0.55, 0.95, 1.0] },
                    string: if is_light { [0.1, 0.5, 0.1, 1.0] } else { [0.55, 0.85, 0.55, 1.0] },
                    number: if is_light { [0.7, 0.4, 0.0, 1.0] } else { [0.85, 0.65, 0.35, 1.0] },
                    boolean: if is_light { [0.0, 0.4, 0.7, 1.0] } else { [0.45, 0.7, 0.95, 1.0] },
                    null_color: if is_light { [0.5, 0.5, 0.5, 1.0] } else { [0.55, 0.55, 0.6, 1.0] },
                    bracket: muted, punctuation: muted,
                };

                for (vis_idx, tl_idx) in (start..total.min(start + available)).enumerate() {
                    let tl = &tree_lines[tl_idx];
                    let line_y = ly + vis_idx as f32 * cell_h;
                    let indent = content_x + field_pad + tl.depth as f32 * indent_w;

                    if tl.is_expandable {
                        let arrow = if tl.expanded { "\u{25BE}" } else { "\u{25B8}" };
                        render_text_line(&mut batch, atlas, gpu_queue,
                            indent, line_y, arrow, arrow_color, cell_w, cell_h);
                    }
                    let text_x = indent + if tl.is_expandable { cell_w + 2.0 * s } else { 0.0 };

                    let val_color = match tl.value_kind {
                        TreeValueKind::String => jc.string,
                        TreeValueKind::Number => jc.number,
                        TreeValueKind::Boolean => jc.boolean,
                        TreeValueKind::Null => jc.null_color,
                        TreeValueKind::Object | TreeValueKind::Array => muted,
                    };

                    if let Some(ref k) = tl.key {
                        let key_text = format!("{}:", k);
                        render_text_line(&mut batch, atlas, gpu_queue,
                            text_x, line_y, &key_text, jc.key, cell_w, cell_h);
                        let after_key = text_x + (key_text.len() as f32 + 1.0) * cell_w;
                        let preview: String = tl.preview.chars().take(resp_max.saturating_sub(
                            ((after_key - content_x - field_pad) / cell_w) as usize
                        ).max(1)).collect();
                        render_text_line(&mut batch, atlas, gpu_queue,
                            after_key, line_y, &preview, val_color, cell_w, cell_h);
                    } else {
                        let preview: String = tl.preview.chars().take(resp_max).collect();
                        render_text_line(&mut batch, atlas, gpu_queue,
                            text_x, line_y, &preview, val_color, cell_w, cell_h);
                    }
                    if line_y + cell_h * 2.0 > response_bottom { break; }
                }

                if total > available && available > 0 {
                    render_scrollbar(&mut batch, content_x + content_w - 4.0 * s, cy,
                        body_area_h, total, available, start, s, is_light);
                }
            } else {
                // ── Raw view ──
                let lines: Vec<&str> = http.response_body.lines().collect();
                let start = http.scroll_offset.min(lines.len().saturating_sub(1));

                let highlight_color = if is_light { [1.0, 0.85, 0.0, 0.35] } else { [1.0, 0.75, 0.0, 0.25] };
                let current_highlight = if is_light { [1.0, 0.6, 0.0, 0.5] } else { [1.0, 0.55, 0.0, 0.45] };
                let query_char_len = http.search_query.chars().count();

                let is_json = http.response_body.trim_start().starts_with('{')
                    || http.response_body.trim_start().starts_with('[');
                let json_colors = if is_json {
                    Some(JsonColors {
                        key: if is_light { [0.55, 0.1, 0.55, 1.0] } else { [0.65, 0.55, 0.95, 1.0] },
                        string: if is_light { [0.1, 0.5, 0.1, 1.0] } else { [0.55, 0.85, 0.55, 1.0] },
                        number: if is_light { [0.7, 0.4, 0.0, 1.0] } else { [0.85, 0.65, 0.35, 1.0] },
                        boolean: if is_light { [0.0, 0.4, 0.7, 1.0] } else { [0.45, 0.7, 0.95, 1.0] },
                        null_color: if is_light { [0.5, 0.5, 0.5, 1.0] } else { [0.55, 0.55, 0.6, 1.0] },
                        bracket: muted, punctuation: muted,
                    })
                } else { None };

                for (vis_idx, line_idx) in (start..lines.len().min(start + available)).enumerate() {
                    let line = lines[line_idx];
                    let display: String = line.chars().take(resp_max).collect();
                    let line_y = ly + vis_idx as f32 * cell_h;

                    if http.search_active && query_char_len > 0 {
                        for (match_idx, &(ml, mc)) in http.search_matches.iter().enumerate() {
                            if ml == line_idx {
                                let hx = content_x + field_pad + mc as f32 * cell_w;
                                let hw = query_char_len as f32 * cell_w;
                                let hc = if match_idx == http.search_current { current_highlight } else { highlight_color };
                                push_quad(&mut batch.bg_verts, &mut batch.bg_indices, hx, line_y, hw, cell_h, hc);
                            }
                        }
                    }
                    if let Some(ref colors) = json_colors {
                        render_json_line(&mut batch, atlas, gpu_queue,
                            content_x + field_pad, line_y, &display, fg_color, colors, cell_w, cell_h);
                    } else {
                        render_text_line(&mut batch, atlas, gpu_queue,
                            content_x + field_pad, line_y, &display, fg_color, cell_w, cell_h);
                    }
                    if line_y + cell_h * 2.0 > response_bottom { break; }
                }

                if lines.len() > available && available > 0 {
                    render_scrollbar(&mut batch, content_x + content_w - 4.0 * s, cy,
                        body_area_h, lines.len(), available, start, s, is_light);
                }
            }
        }
    } else if http.loading {
        render_text_line(&mut batch, atlas, gpu_queue,
            content_x, cy, "Sending request...", muted, cell_w, cell_h);
    } else if let Some(ref err) = http.error {
        let err_color = [0.95, 0.4, 0.4, 1.0];
        let max_chars = ((content_w) / cell_w) as usize;
        // Show error on multiple lines if needed
        let mut ey = cy;
        let mut remaining = err.as_str();
        while !remaining.is_empty() && ey + cell_h < response_bottom {
            let display: String = remaining.chars().take(max_chars).collect();
            let display_len = display.len();
            render_text_line(&mut batch, atlas, gpu_queue,
                content_x, ey, &display, err_color, cell_w, cell_h);
            remaining = &remaining[display_len..];
            ey += cell_h;
        }
    } else if resp_h > cell_h * 2.0 {
        // Empty state - centered placeholder
        let empty_y = response_top + (resp_h - cell_h) / 2.0;
        let hint = "Cmd+Enter to send";
        let hint_w = hint.len() as f32 * cell_w;
        render_text_line(&mut batch, atlas, gpu_queue,
            content_x + (content_w - hint_w) / 2.0, empty_y, hint, placeholder, cell_w, cell_h);
    }

    batch
}

fn render_text_line(
    batch: &mut DrawBatch,
    atlas: &mut GlyphAtlas,
    gpu_queue: &wgpu::Queue,
    x: f32,
    y: f32,
    text: &str,
    color: [f32; 4],
    cell_w: f32,
    cell_h: f32,
) {
    let mut char_x = x;
    for c in text.chars() {
        if c != ' ' && c != '\0' {
            let glyph = atlas.get_or_rasterize(c, false, false, gpu_queue);
            if glyph.width > 0.0 && glyph.height > 0.0 {
                let gx = char_x + glyph.bearing_x;
                let gy = y + (cell_h - glyph.bearing_y);
                let base = batch.fg_verts.len() as u32;
                batch.fg_verts.extend_from_slice(&[
                    TextVertex { position: [gx, gy], tex_coords: [glyph.tex_x, glyph.tex_y], color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx + glyph.width, gy], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y], color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx + glyph.width, gy + glyph.height], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y + glyph.tex_h], color, bg_color: [0.0; 4] },
                    TextVertex { position: [gx, gy + glyph.height], tex_coords: [glyph.tex_x, glyph.tex_y + glyph.tex_h], color, bg_color: [0.0; 4] },
                ]);
                batch.fg_indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
            }
        }
        char_x += cell_w;
    }
}

fn render_close_button(
    batch: &mut DrawBatch,
    canvas_theme: &CanvasTheme,
    tx: f32, ty: f32, tw: f32, bar_h: f32, cell_h: f32, s: f32,
) {
    let close_size = 28.0 * s;
    let close_margin = 8.0 * s;
    let x_size = 8.0 * s;
    let x_thick = 1.5 * s;
    let close_cx = tx + tw - close_size / 2.0 - close_margin;
    let close_cy = ty + (bar_h - cell_h) / 2.0 + cell_h * 0.75;
    let x_color = canvas_theme.title_unfocused.to_array();
    let half = x_size / 2.0;
    let steps = 6i32;
    for i in -steps..=steps {
        let frac = i as f32 / steps as f32;
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            close_cx + frac * half - x_thick / 2.0, close_cy + frac * half - x_thick / 2.0,
            x_thick, x_thick, x_color);
    }
    for i in -steps..=steps {
        let frac = i as f32 / steps as f32;
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            close_cx - frac * half - x_thick / 2.0, close_cy + frac * half - x_thick / 2.0,
            x_thick, x_thick, x_color);
    }
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
        if title_x + cell_w > tx + tw - 10.0 * s { break; }
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

/// Build a toast overlay batch. Called separately so it renders on top of everything.
pub fn build_toast_batch(
    http: &HttpPane,
    tile_x: f32, tile_y: f32, tile_w: f32,
    atlas: &mut GlyphAtlas,
    theme: &Theme,
    canvas_theme: &CanvasTheme,
    gpu_queue: &wgpu::Queue,
    scale_factor: f32,
    bar_h: f32,
    padding: f32,
) -> Option<DrawBatch> {
    if !http.toast_visible() { return None; }
    let msg = http.toast_message.as_ref()?;
    let s = scale_factor;
    let cell_w = atlas.cell_width;
    let cell_h = atlas.cell_height;
    let tile_bg = theme.background.to_array();
    let fg_color = theme.foreground.to_array();
    let border_color = canvas_theme.tile_border.to_array();

    let elapsed_ms = http.toast_time.map(|t| t.elapsed().as_millis() as f32).unwrap_or(0.0);
    let alpha = if elapsed_ms < 150.0 {
        elapsed_ms / 150.0
    } else if elapsed_ms > 1000.0 {
        1.0 - ((elapsed_ms - 1000.0) / 500.0).min(1.0)
    } else {
        1.0
    };
    let slide = if elapsed_ms < 150.0 {
        (1.0 - elapsed_ms / 150.0) * 6.0 * s
    } else {
        0.0
    };

    let mut batch = DrawBatch::new();

    let check = "\u{2713}";
    let toast_pad_x = 14.0 * s;
    let toast_pad_y = 7.0 * s;
    let full_w = (check.len() + 1 + msg.len()) as f32 * cell_w + toast_pad_x * 2.0;
    let toast_h = cell_h + toast_pad_y * 2.0;
    let toast_x = tile_x + (tile_w - full_w) / 2.0;
    let toast_y = tile_y + bar_h + padding * 0.5 + slide;
    let toast_r = toast_h / 2.0;
    let bw = s;

    // Border
    let bc = [border_color[0], border_color[1], border_color[2], alpha];
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        toast_x - bw, toast_y - bw, full_w + bw * 2.0, toast_h + bw * 2.0,
        full_w + bw * 2.0, toast_h + bw * 2.0, toast_r + bw, bc);

    // Solid bg
    let bg = [tile_bg[0], tile_bg[1], tile_bg[2], alpha];
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        toast_x, toast_y, full_w, toast_h, full_w, toast_h, toast_r, bg);

    // ✓ icon
    let check_color = [0.35, 0.85, 0.5, alpha];
    render_text_line(&mut batch, atlas, gpu_queue,
        toast_x + toast_pad_x, toast_y + toast_pad_y, check, check_color, cell_w, cell_h);

    // Text
    let msg_x = toast_x + toast_pad_x + 2.0 * cell_w;
    let fg = [fg_color[0], fg_color[1], fg_color[2], alpha];
    render_text_line(&mut batch, atlas, gpu_queue,
        msg_x, toast_y + toast_pad_y, msg, fg, cell_w, cell_h);

    Some(batch)
}

fn render_scrollbar(
    batch: &mut DrawBatch, x: f32, top_y: f32, area_h: f32,
    total: usize, visible: usize, start: usize, s: f32, is_light: bool,
) {
    let bar_h = (area_h * (visible as f32 / total as f32)).max(12.0 * s);
    let bar_y = top_y + area_h * (start as f32 / total as f32);
    let bar_color = if is_light { [0.0, 0.0, 0.0, 0.15] } else { [1.0, 1.0, 1.0, 0.15] };
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        x, bar_y, 3.0 * s, bar_h, 3.0 * s, bar_h, 1.5 * s, bar_color);
}

struct JsonColors {
    key: [f32; 4],
    string: [f32; 4],
    number: [f32; 4],
    boolean: [f32; 4],
    null_color: [f32; 4],
    bracket: [f32; 4],
    punctuation: [f32; 4],
}

/// Render a single line of JSON-formatted text with syntax highlighting.
/// This is a simple line-level colorizer that doesn't track state across lines.
fn render_json_line(
    batch: &mut DrawBatch,
    atlas: &mut GlyphAtlas,
    gpu_queue: &wgpu::Queue,
    x: f32,
    y: f32,
    line: &str,
    default_color: [f32; 4],
    colors: &JsonColors,
    cell_w: f32,
    cell_h: f32,
) {
    let trimmed = line.trim_start();

    // Determine if this line starts with a key (e.g. `  "key": value`)
    // A key line: after trimming, starts with `"` and has `":` pattern
    let is_key_line = trimmed.starts_with('"')
        && (trimmed.contains("\": ") || trimmed.contains("\":"));

    let mut char_x = x;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let color = if c == '"' {
            // String or key — scan to closing quote
            let start = i;
            i += 1; // skip opening quote
            // Find closing quote (handle escaped quotes)
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 2; // skip escaped char
                } else if chars[i] == '"' {
                    i += 1; // include closing quote
                    break;
                } else {
                    i += 1;
                }
            }

            // Determine if this is a key or a value string
            let str_color = if is_key_line && start == line.len() - trimmed.len() {
                // First quoted string on a key line = key
                colors.key
            } else {
                colors.string
            };

            // Render all chars in this string with the same color
            for j in start..i {
                render_char(batch, atlas, gpu_queue, char_x, y, chars[j], str_color, cell_w, cell_h);
                char_x += cell_w;
            }
            continue;
        } else if c.is_ascii_digit() || (c == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            // Number — scan digits, dots, e, E, +, -
            let start = i;
            if c == '-' { i += 1; }
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'e' || chars[i] == 'E' || chars[i] == '+' || chars[i] == '-') {
                i += 1;
                if i > start + 1 && (chars[i - 1] == '+' || chars[i - 1] == '-') && !matches!(chars.get(i.wrapping_sub(2)), Some('e' | 'E')) {
                    break;
                }
            }
            for j in start..i {
                render_char(batch, atlas, gpu_queue, char_x, y, chars[j], colors.number, cell_w, cell_h);
                char_x += cell_w;
            }
            continue;
        } else if trimmed_starts_keyword(&chars, i, "true") || trimmed_starts_keyword(&chars, i, "false") {
            let kw_len = if chars[i] == 't' { 4 } else { 5 };
            for j in i..i + kw_len {
                render_char(batch, atlas, gpu_queue, char_x, y, chars[j], colors.boolean, cell_w, cell_h);
                char_x += cell_w;
            }
            i += kw_len;
            continue;
        } else if trimmed_starts_keyword(&chars, i, "null") {
            for j in i..i + 4 {
                render_char(batch, atlas, gpu_queue, char_x, y, chars[j], colors.null_color, cell_w, cell_h);
                char_x += cell_w;
            }
            i += 4;
            continue;
        } else if matches!(c, '{' | '}' | '[' | ']') {
            colors.bracket
        } else if matches!(c, ':' | ',') {
            colors.punctuation
        } else {
            default_color
        };

        render_char(batch, atlas, gpu_queue, char_x, y, c, color, cell_w, cell_h);
        char_x += cell_w;
        i += 1;
    }
}

#[inline]
fn render_char(
    batch: &mut DrawBatch,
    atlas: &mut GlyphAtlas,
    gpu_queue: &wgpu::Queue,
    x: f32, y: f32,
    c: char,
    color: [f32; 4],
    _cell_w: f32, cell_h: f32,
) {
    if c != ' ' && c != '\0' {
        let glyph = atlas.get_or_rasterize(c, false, false, gpu_queue);
        if glyph.width > 0.0 && glyph.height > 0.0 {
            let gx = x + glyph.bearing_x;
            let gy = y + (cell_h - glyph.bearing_y);
            let base = batch.fg_verts.len() as u32;
            batch.fg_verts.extend_from_slice(&[
                TextVertex { position: [gx, gy], tex_coords: [glyph.tex_x, glyph.tex_y], color, bg_color: [0.0; 4] },
                TextVertex { position: [gx + glyph.width, gy], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y], color, bg_color: [0.0; 4] },
                TextVertex { position: [gx + glyph.width, gy + glyph.height], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y + glyph.tex_h], color, bg_color: [0.0; 4] },
                TextVertex { position: [gx, gy + glyph.height], tex_coords: [glyph.tex_x, glyph.tex_y + glyph.tex_h], color, bg_color: [0.0; 4] },
            ]);
            batch.fg_indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
        }
    }
}

/// Check if chars starting at `i` match a keyword exactly (not followed by alphanumeric).
fn trimmed_starts_keyword(chars: &[char], i: usize, keyword: &str) -> bool {
    let kw: Vec<char> = keyword.chars().collect();
    if i + kw.len() > chars.len() { return false; }
    for (j, &kc) in kw.iter().enumerate() {
        if chars[i + j] != kc { return false; }
    }
    // Must not be followed by an alphanumeric char
    if i + kw.len() < chars.len() && chars[i + kw.len()].is_alphanumeric() {
        return false;
    }
    true
}
