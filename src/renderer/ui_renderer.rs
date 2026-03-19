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
    tile_count: usize,
    stats_hovered: bool,
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

    // ── HTTP button (bottom-left, next to info icon) ──
    {
        let icon_size = btn_w;
        let http_x = pan.0 + margin + icon_size + gap * 2.0;
        let http_y = sh - margin - icon_size;

        // Border + background
        push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
            http_x - bw, http_y - bw, icon_size + bw * 2.0, icon_size + bw * 2.0,
            icon_size + bw * 2.0, icon_size + bw * 2.0, radius + bw, btn_border);
        push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
            http_x, http_y, icon_size, icon_size, icon_size, icon_size, radius, btn_bg);

        // HTTP icon: draw "{}" braces
        let hcx = http_x + icon_size / 2.0;
        let hcy = http_y + icon_size / 2.0;
        let http_icon_color = [0.9, 0.5, 0.2, 1.0]; // orange to match HTTP tile dot
        let brace_h = 10.0 * s * z;
        let brace_w = 2.0 * s * z;
        let brace_offset = 5.0 * s * z;
        let hook_w = 3.0 * s * z;
        // Left brace {
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            hcx - brace_offset, hcy - brace_h / 2.0, brace_w, brace_h, http_icon_color);
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            hcx - brace_offset, hcy - brace_h / 2.0, hook_w, line_w, http_icon_color);
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            hcx - brace_offset, hcy + brace_h / 2.0 - line_w, hook_w, line_w, http_icon_color);
        // Right brace }
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            hcx + brace_offset - brace_w, hcy - brace_h / 2.0, brace_w, brace_h, http_icon_color);
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            hcx + brace_offset - hook_w, hcy - brace_h / 2.0, hook_w, line_w, http_icon_color);
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            hcx + brace_offset - hook_w, hcy + brace_h / 2.0 - line_w, hook_w, line_w, http_icon_color);
    }

    // ── Stats info icon (bottom-left corner) ──
    {
        let icon_size = btn_w; // same size as other buttons
        let icon_x = pan.0 + margin;
        let icon_y = sh - margin - icon_size;

        // Icon button: border + background
        push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
            icon_x - bw, icon_y - bw, icon_size + bw * 2.0, icon_size + bw * 2.0,
            icon_size + bw * 2.0, icon_size + bw * 2.0, radius + bw, btn_border);
        push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
            icon_x, icon_y, icon_size, icon_size, icon_size, icon_size, radius, btn_bg);

        // "i" icon: dot + vertical bar
        let icx = icon_x + icon_size / 2.0;
        let icy = icon_y + icon_size / 2.0;
        let dot_r = 1.5 * s * z;
        // dot
        push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
            icx - dot_r, icy - 7.0 * s * z, dot_r * 2.0, dot_r * 2.0,
            dot_r * 2.0, dot_r * 2.0, dot_r, icon_color);
        // stem
        let stem_h = 8.0 * s * z;
        let stem_w = line_w;
        push_quad(&mut batch.bg_verts, &mut batch.bg_indices,
            icx - stem_w / 2.0, icy - 2.0 * s * z, stem_w, stem_h, icon_color);

        // ── Tooltip on hover ──
        if stats_hovered {
            let stats = CACHED_STATS.with(|c| c.borrow().clone());
            let label = format!("{} tiles  {}  {}", tile_count, stats.cpu, stats.mem);
            let char_w = atlas.cell_width * z;
            let char_h = atlas.cell_height * z;
            let pad_x = 10.0 * s * z;
            let pad_y = 6.0 * s * z;
            let pill_w = pad_x * 2.0 + label.len() as f32 * char_w;
            let pill_h = pad_y * 2.0 + char_h;
            let pill_x = icon_x + icon_size + 8.0 * s * z;
            let pill_y = icon_y + (icon_size - pill_h) / 2.0;

            // Border
            push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
                pill_x - bw, pill_y - bw, pill_w + bw * 2.0, pill_h + bw * 2.0,
                pill_w + bw * 2.0, pill_h + bw * 2.0, radius + bw, btn_border);
            // Background
            push_rounded_quad(&mut batch.rounded_verts, &mut batch.rounded_indices,
                pill_x, pill_y, pill_w, pill_h, pill_w, pill_h, radius, btn_bg);

            // Text
            let label_color = canvas_theme.label.to_array();
            let mut lx = pill_x + pad_x;
            let ly = pill_y + pad_y;
            for c in label.chars() {
                if c != ' ' {
                    let glyph = atlas.get_or_rasterize_ui(c, gpu_queue);
                    if glyph.width > 0.0 && glyph.height > 0.0 {
                        let gw = glyph.width * z;
                        let gh = glyph.height * z;
                        let gx = lx + glyph.bearing_x * z;
                        let gy = ly + (char_h - glyph.bearing_y * z);
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
        }
    }

    batch
}

struct CachedStats {
    cpu: String,
    mem: String,
    last_update: std::time::Instant,
}

impl Clone for CachedStats {
    fn clone(&self) -> Self {
        Self { cpu: self.cpu.clone(), mem: self.mem.clone(), last_update: self.last_update }
    }
}

thread_local! {
    static CACHED_STATS: std::cell::RefCell<CachedStats> = std::cell::RefCell::new(CachedStats {
        cpu: String::from("CPU --"),
        mem: String::from("MEM --"),
        last_update: std::time::Instant::now(),
    });
}

fn refresh_stats() {
    CACHED_STATS.with(|c| {
        let mut stats = c.borrow_mut();
        if stats.last_update.elapsed().as_secs() < 3 { return; }
        stats.last_update = std::time::Instant::now();

        let pid = std::process::id();

        // App CPU and RSS via ps
        if let Ok(o) = std::process::Command::new("ps")
            .args(["-o", "%cpu=,rss=", "-p", &pid.to_string()])
            .output()
        {
            if let Ok(s) = String::from_utf8(o.stdout) {
                let parts: Vec<&str> = s.trim().split_whitespace().collect();
                if parts.len() >= 2 {
                    stats.cpu = format!("CPU {}%", parts[0].split('.').next().unwrap_or("0"));
                    if let Ok(rss_kb) = parts[1].parse::<u64>() {
                        let mb = rss_kb / 1024;
                        stats.mem = format!("MEM {}MB", mb);
                    }
                }
            }
        }
    });
}

/// Call this once per frame to keep stats fresh.
pub fn update_stats() {
    refresh_stats();
}
