use crate::postgres_pane::{PgField, PgStatus, PostgresPane};
use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::draw_helpers::{push_quad, push_rounded_quad, DrawBatch};
use crate::renderer::text::TextVertex;
use crate::ui::canvas::Tile;
use crate::ui::canvas_theme::CanvasTheme;
use crate::ui::theme::Theme;

pub fn build_postgres_tile_batch(
    tile: &Tile,
    pg: &mut PostgresPane,
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

    // ── Tile chrome ──
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx - bw, ty - bw, tw + bw2, total_h + bw2, tw + bw2, total_h + bw2, br, border_color);
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx, ty, tw, total_h, tw, total_h, corner_radius, tile_bg);
    let bar_color = canvas_theme.tile_bar.to_array();
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        tx, ty, tw, bar_h, tw, total_h, corner_radius, bar_color);

    // Resize handle
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

    // Active dot (green for postgres)
    if is_focused {
        let dot_r = 3.5 * s;
        let dot_cx = tx + 14.0 * s;
        let title_y = ty + (bar_h - cell_h) / 2.0;
        let dot_cy = title_y + cell_h * 0.65;
        let dot_color = pg.status.color();
        let outer_r = dot_r + 1.2 * s;
        let border_c = [dot_color[0] * 1.2, dot_color[1] * 1.2, dot_color[2] * 1.2, 1.0];
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

    macro_rules! draw_field_border {
        ($bx:expr, $by:expr, $bw_:expr, $bh:expr, $focused:expr) => {
            let bc = if $focused { field_border_active } else { field_border };
            let bt = bw;
            push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
                $bx, $by, $bw_, $bh, $bw_, $bh, field_r, bc);
            push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
                $bx + bt, $by + bt, $bw_ - bt * 2.0, $bh - bt * 2.0,
                $bw_ - bt * 2.0, $bh - bt * 2.0, (field_r - bt).max(0.0), tile_bg);
        };
    }

    // ── Connection string row ──
    let conn_x = content_x;
    let connect_label = if matches!(pg.status, PgStatus::Connected) { "Connected" } else if matches!(pg.status, PgStatus::Connecting) { "..." } else { "Connect" };
    let connect_btn_w = (connect_label.len() as f32 + 2.0) * cell_w;
    let conn_field_w = content_w - gap * 0.5 - connect_btn_w;

    let conn_focused = pg.focus_field == PgField::Connection && is_focused;
    draw_field_border!(conn_x, cy, conn_field_w, field_h, conn_focused);

    let conn_text_x = conn_x + field_pad;
    let conn_text_y = cy + field_pad;
    let max_conn_chars = ((conn_field_w - field_pad * 2.0) / cell_w) as usize;

    if pg.connection_string.is_empty() {
        render_text_line(&mut batch, atlas, gpu_queue,
            conn_text_x, conn_text_y, "postgres://user:pass@host:5432/db", placeholder, cell_w, cell_h);
    } else {
        let display = pg.display_connection_string();
        let cursor_chars = pg.connection_string[..pg.conn_cursor].chars().count();
        let scroll = if cursor_chars > max_conn_chars.saturating_sub(2) {
            cursor_chars.saturating_sub(max_conn_chars.saturating_sub(2))
        } else { 0 };
        let display_chars: String = display.chars().skip(scroll).take(max_conn_chars).collect();
        render_text_line(&mut batch, atlas, gpu_queue,
            conn_text_x, conn_text_y, &display_chars, fg_color, cell_w, cell_h);
        if conn_focused {
            let vis_cursor = cursor_chars.saturating_sub(scroll);
            let cx = conn_text_x + vis_cursor as f32 * cell_w;
            if pg.cursor_renderer.visible && (!pg.cursor_renderer.blink || pg.cursor_renderer.blink_on()) {
                push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                    cx, conn_text_y, 2.0 * s, cell_h, theme.cursor.to_array());
            }
        }
    }

    // Connect button
    let connect_x = conn_x + conn_field_w + gap * 0.5;
    let connect_bg = if matches!(pg.status, PgStatus::Connected) {
        [0.15, 0.4, 0.2, 1.0]
    } else if matches!(pg.status, PgStatus::Connecting) {
        [0.25, 0.25, 0.3, 1.0]
    } else {
        [0.15, 0.3, 0.7, 1.0]
    };
    let actual_btn_w = (connect_label.len() as f32 + 2.0) * cell_w;
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        connect_x, cy, actual_btn_w, field_h, actual_btn_w, field_h, field_h / 2.0, connect_bg);
    render_text_line(&mut batch, atlas, gpu_queue,
        connect_x + (actual_btn_w - connect_label.len() as f32 * cell_w) / 2.0,
        cy + field_pad, connect_label, [1.0, 1.0, 1.0, 0.95], cell_w, cell_h);

    cy += field_h + gap;

    // ── Query editor ──
    let query_focused = pg.focus_field == PgField::Query && is_focused;

    render_text_line(&mut batch, atlas, gpu_queue,
        content_x + 2.0 * s, cy, "Query", muted, cell_w, cell_h);

    // Hint on the right
    if query_focused {
        let hint = "Cmd+Enter: run";
        let hint_w = hint.len() as f32 * cell_w;
        render_text_line(&mut batch, atlas, gpu_queue,
            content_x + content_w - hint_w, cy, hint, placeholder, cell_w, cell_h);
    }
    cy += cell_h + 4.0 * s;

    let max_query_lines = 10;
    let query_lines: Vec<&str> = if pg.query.is_empty() { vec![""] } else { pg.query.lines().collect() };
    let total_query_lines = query_lines.len().max(1);
    let visible_lines = total_query_lines.min(max_query_lines);
    let query_h = visible_lines as f32 * cell_h + field_pad * 2.0;

    draw_field_border!(content_x, cy, content_w, query_h, query_focused);

    let query_text_x = content_x + field_pad;
    let query_max_chars = ((content_w - field_pad * 2.0) / cell_w) as usize;

    // Scroll to keep cursor visible
    let cursor_row = if query_focused {
        pg.query[..pg.query_cursor].matches('\n').count()
    } else { 0 };
    let query_scroll = if total_query_lines <= max_query_lines {
        0
    } else if cursor_row >= max_query_lines {
        (cursor_row - max_query_lines + 1).min(total_query_lines - max_query_lines)
    } else {
        0
    };

    if pg.query.is_empty() {
        render_text_line(&mut batch, atlas, gpu_queue,
            query_text_x, cy + field_pad, "SELECT * FROM ...", placeholder, cell_w, cell_h);
    } else {
        // SQL keyword colors
        let kw_color = if is_light { [0.0, 0.3, 0.7, 1.0] } else { [0.45, 0.7, 0.95, 1.0] };
        let str_color = if is_light { [0.1, 0.5, 0.1, 1.0] } else { [0.55, 0.85, 0.55, 1.0] };
        let num_color = if is_light { [0.7, 0.4, 0.0, 1.0] } else { [0.85, 0.65, 0.35, 1.0] };

        for (vis_i, line_i) in (query_scroll..total_query_lines.min(query_scroll + max_query_lines)).enumerate() {
            if line_i < query_lines.len() {
                let display: String = query_lines[line_i].chars().take(query_max_chars).collect();
                let line_y = cy + field_pad + vis_i as f32 * cell_h;
                render_sql_line(&mut batch, atlas, gpu_queue,
                    query_text_x, line_y, &display, fg_color, kw_color, str_color, num_color, cell_w, cell_h);
            }
        }
    }

    // Query cursor
    if query_focused {
        let before = &pg.query[..pg.query_cursor];
        let cr = before.matches('\n').count();
        let last_nl = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let cc = before[last_nl..].chars().count();
        let vis_row = cr.saturating_sub(query_scroll);
        if vis_row < max_query_lines {
            let bx = query_text_x + cc as f32 * cell_w;
            let by = cy + field_pad + vis_row as f32 * cell_h;
            if pg.cursor_renderer.visible && (!pg.cursor_renderer.blink || pg.cursor_renderer.blink_on()) {
                push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                    bx, by, 2.0 * s, cell_h, theme.cursor.to_array());
            }
        }
    }

    cy += query_h + gap;

    // ── Separator ──
    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
        content_x, cy, content_w, bw, field_border);
    cy += bw + gap;

    // ── Results area ──
    let results_top = cy;
    let results_bottom = ty + total_h - pad;
    let results_h = results_bottom - results_top;

    if !pg.columns.is_empty() && results_h > cell_h {
        // Results header: row count + time
        let info = format!("{} rows", pg.row_count);
        render_text_line(&mut batch, atlas, gpu_queue,
            content_x, cy, &info, muted, cell_w, cell_h);

        let mut info_x = content_x + (info.len() as f32 + 1.0) * cell_w;
        if let Some(ms) = pg.exec_time_ms {
            let time_str = if ms < 1000 { format!("{}ms", ms) } else { format!("{:.1}s", ms as f64 / 1000.0) };
            render_text_line(&mut batch, atlas, gpu_queue,
                info_x, cy, &format!("\u{00B7} {}", time_str), muted, cell_w, cell_h);
            info_x += (time_str.len() as f32 + 3.0) * cell_w;
        }
        let _ = info_x; // suppress warning

        cy += cell_h + gap * 0.5;

        // Table area
        let table_h = results_bottom - cy;
        if table_h > cell_h {
            draw_field_border!(content_x, cy, content_w, table_h, false);

            let table_x = content_x + field_pad;
            let table_content_w = content_w - field_pad * 2.0;
            let available_lines = ((table_h - field_pad) / cell_h).max(0.0) as usize;
            let table_ly = cy + field_pad * 0.5;

            let col_gap = 2; // chars between columns
            let scroll_x = pg.results_scroll_x;
            let visible_chars = (table_content_w / cell_w) as usize;

            // Helper: render a table row with horizontal scroll
            let render_table_row = |batch: &mut DrawBatch, atlas: &mut GlyphAtlas, gpu_queue: &wgpu::Queue,
                                     row_y: f32, cells: &[(&str, [f32; 4])], col_widths: &[usize]| {
                let mut cx_offset = 0usize;
                for (i, (text, color)) in cells.iter().enumerate() {
                    let w = col_widths.get(i).copied().unwrap_or(10);
                    let col_start = cx_offset;
                    let col_end = cx_offset + w;
                    cx_offset += w + col_gap;

                    // Skip columns entirely before scroll window
                    if col_end <= scroll_x { continue; }
                    // Stop if column starts beyond visible area
                    if col_start >= scroll_x + visible_chars { break; }

                    // Calculate visible portion of this column
                    let text_chars: Vec<char> = text.chars().collect();
                    let padded_len = w;
                    let vis_start = if col_start >= scroll_x { 0 } else { scroll_x - col_start };
                    let vis_end = padded_len.min(scroll_x + visible_chars - col_start);
                    let pixel_x = table_x + (col_start as f32 - scroll_x as f32 + vis_start as f32).max(0.0) * cell_w;

                    // Build visible slice with padding
                    let mut display = String::new();
                    for ci in vis_start..vis_end {
                        if ci < text_chars.len() {
                            display.push(text_chars[ci]);
                        } else {
                            display.push(' ');
                        }
                    }

                    render_text_line(batch, atlas, gpu_queue,
                        pixel_x, row_y, &display, *color, cell_w, cell_h);
                }
            };

            // Render header
            if available_lines > 0 {
                let header_color = if is_light { [0.3, 0.3, 0.5, 1.0] } else { [0.6, 0.6, 0.85, 1.0] };
                let header_cells: Vec<(&str, [f32; 4])> = pg.columns.iter()
                    .map(|c| (c.as_str(), header_color)).collect();
                render_table_row(&mut batch, atlas, gpu_queue,
                    table_ly, &header_cells, &pg.col_widths);
            }

            // Separator line under header
            if available_lines > 1 {
                let sep_y = table_ly + cell_h;
                let sep_color = field_border;
                push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                    table_x, sep_y, table_content_w, bw, sep_color);
            }

            // Data rows
            let data_start = 2;
            let data_available = available_lines.saturating_sub(data_start);
            let scroll = pg.results_scroll.min(pg.rows.len().saturating_sub(1));
            let null_color = if is_light { [0.5, 0.5, 0.5, 0.6] } else { [0.5, 0.5, 0.5, 0.6] };

            for (vis_i, row_i) in (scroll..pg.rows.len().min(scroll + data_available)).enumerate() {
                let row = &pg.rows[row_i];
                let row_y = table_ly + (data_start + vis_i) as f32 * cell_h;

                // Alternate row background
                if vis_i % 2 == 1 {
                    let alt_bg = if is_light { [0.0, 0.0, 0.0, 0.02] } else { [1.0, 1.0, 1.0, 0.02] };
                    push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
                        table_x, row_y, table_content_w, cell_h, alt_bg);
                }

                let row_cells: Vec<(&str, [f32; 4])> = row.iter()
                    .map(|v| {
                        let color = if v == "NULL" { null_color } else { fg_color };
                        (v.as_str(), color)
                    }).collect();
                render_table_row(&mut batch, atlas, gpu_queue,
                    row_y, &row_cells, &pg.col_widths);

                if row_y + cell_h * 2.0 > results_bottom { break; }
            }

            // Vertical scrollbar
            if pg.rows.len() > data_available && data_available > 0 {
                render_scrollbar(&mut batch, content_x + content_w - 4.0 * s, cy,
                    table_h, pg.rows.len(), data_available, scroll, s, is_light);
            }

            // Horizontal scrollbar
            let total_w = pg.total_table_width();
            if total_w > visible_chars && visible_chars > 0 {
                let hbar_thickness = 8.0 * s;
                let hbar_y = cy + table_h - hbar_thickness - 2.0 * s;
                let scrollable_w = table_content_w - 10.0 * s; // leave margin for vertical scrollbar
                let hbar_w = (scrollable_w * (visible_chars as f32 / total_w as f32)).max(24.0 * s);
                let max_scroll = total_w.saturating_sub(visible_chars);
                let scroll_ratio = if max_scroll > 0 { scroll_x as f32 / max_scroll as f32 } else { 0.0 };
                let hbar_x = table_x + scroll_ratio * (scrollable_w - hbar_w);
                let hbar_color = if is_light { [0.0, 0.0, 0.0, 0.2] } else { [1.0, 1.0, 1.0, 0.2] };
                push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
                    hbar_x, hbar_y, hbar_w, hbar_thickness, hbar_w, hbar_thickness, hbar_thickness / 2.0, hbar_color);
            }
        }
    } else if matches!(pg.status, PgStatus::Executing) {
        render_text_line(&mut batch, atlas, gpu_queue,
            content_x, cy, "Executing query...", muted, cell_w, cell_h);
    } else if let Some(ref err) = pg.error {
        let err_color = [0.95, 0.4, 0.4, 1.0];
        let mut ey = cy;
        let mut remaining = err.as_str();
        while !remaining.is_empty() && ey + cell_h < results_bottom {
            let display: String = remaining.chars().take(max_chars).collect();
            let display_len = display.len();
            render_text_line(&mut batch, atlas, gpu_queue,
                content_x, ey, &display, err_color, cell_w, cell_h);
            remaining = &remaining[display_len..];
            ey += cell_h;
        }
    } else if results_h > cell_h * 2.0 {
        let empty_y = results_top + (results_h - cell_h) / 2.0;
        let hint = if matches!(pg.status, PgStatus::Connected) {
            "Cmd+Enter to execute"
        } else {
            "Connect to a database"
        };
        let hint_w = hint.len() as f32 * cell_w;
        render_text_line(&mut batch, atlas, gpu_queue,
            content_x + (content_w - hint_w) / 2.0, empty_y, hint, placeholder, cell_w, cell_h);
    }

    batch
}

/// SQL keywords for syntax highlighting.
const SQL_KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "INSERT", "INTO", "UPDATE", "DELETE",
    "CREATE", "DROP", "ALTER", "TABLE", "INDEX", "VIEW",
    "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "CROSS", "FULL",
    "ON", "AS", "AND", "OR", "NOT", "IN", "IS", "NULL", "LIKE",
    "ORDER", "BY", "GROUP", "HAVING", "LIMIT", "OFFSET",
    "DISTINCT", "UNION", "ALL", "EXISTS", "BETWEEN", "CASE",
    "WHEN", "THEN", "ELSE", "END", "SET", "VALUES", "RETURNING",
    "WITH", "RECURSIVE", "ASC", "DESC", "COUNT", "SUM", "AVG",
    "MIN", "MAX", "COALESCE", "CAST", "TRUE", "FALSE",
    "BEGIN", "COMMIT", "ROLLBACK", "EXPLAIN", "ANALYZE",
];

fn render_sql_line(
    batch: &mut DrawBatch,
    atlas: &mut GlyphAtlas,
    gpu_queue: &wgpu::Queue,
    x: f32,
    y: f32,
    line: &str,
    default_color: [f32; 4],
    kw_color: [f32; 4],
    str_color: [f32; 4],
    num_color: [f32; 4],
    cell_w: f32,
    cell_h: f32,
) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut char_x = x;

    while i < chars.len() {
        let c = chars[i];

        if c == '\'' {
            // String literal
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\'' {
                    if i + 1 < chars.len() && chars[i + 1] == '\'' {
                        i += 2; // escaped quote
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            for j in start..i {
                render_char(batch, atlas, gpu_queue, char_x, y, chars[j], str_color, cell_w, cell_h);
                char_x += cell_w;
            }
            continue;
        }

        if c == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            // Line comment
            let comment_color = [default_color[0] * 0.5, default_color[1] * 0.5, default_color[2] * 0.5, 0.6];
            for j in i..chars.len() {
                render_char(batch, atlas, gpu_queue, char_x, y, chars[j], comment_color, cell_w, cell_h);
                char_x += cell_w;
            }
            return;
        }

        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            // Number
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            for j in start..i {
                render_char(batch, atlas, gpu_queue, char_x, y, chars[j], num_color, cell_w, cell_h);
                char_x += cell_w;
            }
            continue;
        }

        if c.is_ascii_alphabetic() || c == '_' {
            // Word — check if SQL keyword
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_uppercase();
            let color = if SQL_KEYWORDS.contains(&upper.as_str()) {
                kw_color
            } else {
                default_color
            };
            for j in start..i {
                render_char(batch, atlas, gpu_queue, char_x, y, chars[j], color, cell_w, cell_h);
                char_x += cell_w;
            }
            continue;
        }

        render_char(batch, atlas, gpu_queue, char_x, y, c, default_color, cell_w, cell_h);
        char_x += cell_w;
        i += 1;
    }
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

fn render_scrollbar(
    batch: &mut DrawBatch, x: f32, top_y: f32, area_h: f32,
    total: usize, visible: usize, start: usize, s: f32, is_light: bool,
) {
    let bar_w = 8.0 * s;
    let margin = 2.0 * s;
    let track_h = area_h - margin * 2.0;
    let bar_h = (track_h * (visible as f32 / total as f32)).max(24.0 * s);
    let max_scroll = total.saturating_sub(visible);
    let scroll_ratio = if max_scroll > 0 { start as f32 / max_scroll as f32 } else { 0.0 };
    let bar_y = top_y + margin + scroll_ratio * (track_h - bar_h);
    let bar_color = if is_light { [0.0, 0.0, 0.0, 0.2] } else { [1.0, 1.0, 1.0, 0.2] };
    let bar_x = x - bar_w;
    push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
        bar_x, bar_y, bar_w, bar_h, bar_w, bar_h, bar_w / 2.0, bar_color);
}
