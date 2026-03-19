use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use winit::keyboard::ModifiersState;
use winit::window::Window;

use crate::config::Config;
use crate::input::{completion, keyboard};
use crate::input::history::CommandHistory;
use crate::pane::Pane;
use crate::state::{AppState, TileState};
use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::draw_helpers::DrawBatch;
use crate::renderer::gpu::GpuContext;
use crate::renderer::grid_renderer;
use crate::renderer::text::TextVertex;
use crate::renderer::tile_renderer;
use crate::renderer::ui_renderer;
use crate::ui::canvas::{Canvas, DragMode, TITLE_BAR_HEIGHT};
use crate::ui::canvas_theme::CanvasTheme;
use crate::ui::theme::Theme;

use wgpu::util::DeviceExt;

/// Text selection anchor and endpoint in grid coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Selection {
    pub tile_id: usize,
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
}

impl Selection {
    /// Return (start, end) ordered so start <= end.
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if (self.start_row, self.start_col) <= (self.end_row, self.end_col) {
            ((self.start_row, self.start_col), (self.end_row, self.end_col))
        } else {
            ((self.end_row, self.end_col), (self.start_row, self.start_col))
        }
    }

    /// Check if a cell is within the selection.
    pub fn contains(&self, row: usize, col: usize) -> bool {
        let ((sr, sc), (er, ec)) = self.ordered();
        if row < sr || row > er { return false; }
        if row == sr && row == er { return col >= sc && col <= ec; }
        if row == sr { return col >= sc; }
        if row == er { return col <= ec; }
        true
    }
}

pub struct App {
    pub gpu: GpuContext,
    pub atlas: GlyphAtlas,
    pub theme: Theme,
    pub config: Config,
    pub modifiers: ModifiersState,
    pub scale_factor: f32,

    panes: HashMap<usize, Pane>,
    pub canvas: Canvas,
    pub canvas_zoom: f32,
    pub canvas_pan: (f32, f32),
    panning: Option<(f32, f32)>,
    pub renaming: bool,
    rename_buffer: String,
    pub is_dark: bool,

    pub selection: Option<Selection>,
    selecting: bool,

    bg_pipeline: wgpu::RenderPipeline,
    fg_pipeline: wgpu::RenderPipeline,
    rounded_pipeline: wgpu::RenderPipeline,
    uniform_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    texture_bind_group: wgpu::BindGroup,

    window: Arc<Window>,

    state_dirty: bool,
    last_save: Instant,
    command_history: CommandHistory,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    projection: [[f32; 4]; 4],
}

fn ortho_pan(width: f32, height: f32, pan_x: f32, pan_y: f32) -> [[f32; 4]; 4] {
    [
        [2.0 / width, 0.0, 0.0, 0.0],
        [0.0, -2.0 / height, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-1.0 - 2.0 * pan_x / width, 1.0 + 2.0 * pan_y / height, 0.0, 1.0],
    ]
}

pub enum AppAction {
    None,
    SpawnTile,
    ClosePane,
    Quit,
}

impl App {
    pub async fn new(window: Arc<Window>, config: Config) -> Self {
        let gpu = GpuContext::new(window.clone()).await;
        let size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;

        let atlas = GlyphAtlas::new(
            &gpu.device,
            &gpu.queue,
            config.appearance.font_size,
            scale_factor,
        );

        // Load saved state or create default
        let saved = AppState::load();
        let bar_h = TITLE_BAR_HEIGHT * scale_factor;
        let padding = config.appearance.padding as f32 * scale_factor;
        let mut canvas = Canvas::new();
        let mut panes = HashMap::new();

        let (initial_zoom, initial_pan_x, initial_pan_y, is_dark_init);

        if !saved.tiles.is_empty() {
            // Restore saved tiles
            for ts in saved.tiles {
                let tile_id = canvas.spawn_named(ts.x, ts.y, ts.w, ts.h, ts.name.clone());
                let cols = ((ts.w - padding * 2.0) / atlas.cell_width).max(1.0) as usize;
                let rows = ((ts.h - bar_h - padding * 2.0) / atlas.cell_height).max(1.0) as usize;
                let pane = Pane::new(&config.terminal.shell, cols, rows, config.terminal.cursor_blink);
                panes.insert(tile_id, pane);
            }
            initial_zoom = saved.canvas_zoom;
            initial_pan_x = saved.canvas_pan.0;
            initial_pan_y = saved.canvas_pan.1;
            is_dark_init = saved.is_dark;
        } else {
            // First launch: create default tile (snapped to grid)
            let grid = 24.0 * scale_factor;
            let snap = |v: f32| (v / grid).round() * grid;
            let tw = snap(800.0 * scale_factor);
            let th = snap(800.0 * scale_factor);
            let tx = snap((size.width as f32 - tw) / 2.0);
            let ty = snap((size.height as f32 - th - bar_h) / 2.0);
            let tile_id = canvas.spawn(tx, ty, tw, th);
            let cols = ((tw - padding * 2.0) / atlas.cell_width).max(1.0) as usize;
            let rows = ((th - bar_h - padding * 2.0) / atlas.cell_height).max(1.0) as usize;
            let pane = Pane::new(&config.terminal.shell, cols, rows, config.terminal.cursor_blink);
            panes.insert(tile_id, pane);

            initial_zoom = 1.0;
            let view_w = size.width as f32;
            let view_h = size.height as f32;
            let tile_center_x = tx + tw / 2.0;
            let tile_center_y = ty + (th + bar_h) / 2.0;
            initial_pan_x = tile_center_x - view_w / 2.0;
            initial_pan_y = tile_center_y - view_h / 2.0;
            is_dark_init = true;
        }

        let theme = if is_dark_init { Theme::catppuccin_mocha() } else { Theme::light() };

        // Shaders & pipelines
        let text_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../assets/shaders/text.wgsl").into()),
        });

        let view_w = size.width as f32 / initial_zoom;
        let view_h = size.height as f32 / initial_zoom;
        let uniforms = Uniforms { projection: ortho_pan(view_w, view_h, initial_pan_x, initial_pan_y) };
        let uniform_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("uniforms"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform-bg"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }],
        });

        let texture_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { multisampled: false, view_dimension: wgpu::TextureViewDimension::D2, sample_type: wgpu::TextureSampleType::Float { filterable: true } },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let texture_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture-bg"),
            layout: &texture_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&atlas.view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&atlas.sampler) },
            ],
        });

        let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl"), bind_group_layouts: &[&uniform_bgl, &texture_bgl], push_constant_ranges: &[],
        });
        let format = gpu.format();

        let bg_pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bg"), layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &text_shader, entry_point: Some("vs_main"), buffers: &[TextVertex::layout()], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &text_shader, entry_point: Some("fs_bg_main"), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None, cache: None,
        });
        let fg_pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fg"), layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &text_shader, entry_point: Some("vs_main"), buffers: &[TextVertex::layout()], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &text_shader, entry_point: Some("fs_main"), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None, cache: None,
        });
        let rounded_pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rounded"), layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &text_shader, entry_point: Some("vs_main"), buffers: &[TextVertex::layout()], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &text_shader, entry_point: Some("fs_rounded_bg"), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None, cache: None,
        });

        Self {
            gpu, atlas, theme, config,
            modifiers: ModifiersState::empty(),
            scale_factor,
            panes, canvas,
            canvas_zoom: initial_zoom,
            canvas_pan: (initial_pan_x, initial_pan_y),
            panning: None,
            renaming: false,
            rename_buffer: String::new(),
            is_dark: is_dark_init,
            selection: None,
            selecting: false,
            bg_pipeline, fg_pipeline, rounded_pipeline,
            uniform_bind_group, uniform_buffer, texture_bind_group,
            window,
            state_dirty: false,
            last_save: Instant::now(),
            command_history: CommandHistory::load(),
        }
    }

    fn snap_to_grid(&self, val: f32) -> f32 {
        let grid = 24.0 * self.scale_factor;
        (val / grid).round() * grid
    }

    fn default_tile_size(&self) -> (f32, f32) {
        let tw = self.snap_to_grid(800.0 * self.scale_factor);
        let th = self.snap_to_grid(800.0 * self.scale_factor);
        (tw, th)
    }

    pub fn spawn_tile(&mut self) {
        let size = self.window.inner_size();
        let s = self.scale_factor;
        let (tw, th) = self.default_tile_size();
        let grid = 24.0 * s;
        let offset = (self.canvas.tiles.len() as f32 * grid) % (8.0 * grid);
        let x = self.snap_to_grid(((size.width as f32 - tw) / 2.0 + offset).max(0.0));
        let y = self.snap_to_grid(((size.height as f32 - th) / 2.0 + offset).max(0.0));

        let tile_id = self.canvas.spawn(x, y, tw, th);
        let padding = self.config.appearance.padding as f32 * s;
        let bar_h = TITLE_BAR_HEIGHT * s;
        let cols = ((tw - padding * 2.0) / self.atlas.cell_width).max(1.0) as usize;
        let rows = ((th - bar_h - padding * 2.0) / self.atlas.cell_height).max(1.0) as usize;

        let pane = Pane::new(&self.config.terminal.shell, cols, rows, self.config.terminal.cursor_blink);
        self.panes.insert(tile_id, pane);
        self.mark_dirty();
    }

    /// Spawn a new tile centered at a canvas position.
    pub fn spawn_tile_at(&mut self, cx: f32, cy: f32) {
        let s = self.scale_factor;
        let (tw, th) = self.default_tile_size();
        let x = self.snap_to_grid(cx - tw / 2.0);
        let y = self.snap_to_grid(cy - th / 2.0);

        let tile_id = self.canvas.spawn(x, y, tw, th);
        let padding = self.config.appearance.padding as f32 * s;
        let bar_h = TITLE_BAR_HEIGHT * s;
        let cols = ((tw - padding * 2.0) / self.atlas.cell_width).max(1.0) as usize;
        let rows = ((th - bar_h - padding * 2.0) / self.atlas.cell_height).max(1.0) as usize;

        let pane = Pane::new(&self.config.terminal.shell, cols, rows, self.config.terminal.cursor_blink);
        self.panes.insert(tile_id, pane);
        self.mark_dirty();
    }

    pub fn close_focused(&mut self) {
        if let Some(id) = self.canvas.focused_id() {
            self.close_tile(id);
        }
    }

    pub fn close_tile(&mut self, id: usize) {
        self.canvas.remove(id);
        self.panes.remove(&id);
        self.mark_dirty();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
        self.update_projection();
    }

    fn resize_pane_to_tile(&mut self, tile_id: usize) {
        let Some(tile) = self.canvas.tile(tile_id) else { return };
        let padding = self.config.appearance.padding as f32 * self.scale_factor;
        let bar_h = TITLE_BAR_HEIGHT * self.scale_factor;
        let cols = ((tile.w - padding * 2.0) / self.atlas.cell_width).max(1.0) as usize;
        let rows = ((tile.h - bar_h - padding * 2.0) / self.atlas.cell_height).max(1.0) as usize;
        if let Some(pane) = self.panes.get_mut(&tile_id) {
            pane.resize(cols, rows);
        }
    }

    /// Zoom centered on a screen-space point (physical pixels).
    pub fn toggle_theme(&mut self) {
        self.is_dark = !self.is_dark;
        self.theme = if self.is_dark {
            Theme::catppuccin_mocha()
        } else {
            Theme::light()
        };
        set_macos_appearance(&self.window, self.is_dark);
        self.mark_dirty();
    }

    pub fn zoom_at(&mut self, screen_x: f32, screen_y: f32, delta: f32) {
        let old_zoom = self.canvas_zoom;
        let new_zoom = (self.canvas_zoom + delta).clamp(0.3, 2.0);
        if (new_zoom - old_zoom).abs() < 0.001 { return; }

        // Canvas point under cursor before zoom
        let cx = screen_x / old_zoom + self.canvas_pan.0;
        let cy = screen_y / old_zoom + self.canvas_pan.1;

        self.canvas_zoom = new_zoom;

        // Adjust pan so the same canvas point stays under cursor
        self.canvas_pan.0 = cx - screen_x / new_zoom;
        self.canvas_pan.1 = cy - screen_y / new_zoom;

        // Re-rasterize glyphs at new zoom for sharp text
        self.atlas.set_zoom(new_zoom, &self.gpu.queue);

        self.update_projection();
        self.mark_dirty();
    }

    pub fn zoom_in(&mut self) {
        let size = self.window.inner_size();
        let cx = size.width as f32 / 2.0;
        let cy = size.height as f32 / 2.0;
        self.zoom_at(cx, cy, 0.1);
    }

    pub fn zoom_out(&mut self) {
        let size = self.window.inner_size();
        let cx = size.width as f32 / 2.0;
        let cy = size.height as f32 / 2.0;
        self.zoom_at(cx, cy, -0.1);
    }

    pub fn update_projection(&mut self) {
        let size = self.window.inner_size();
        let w = size.width as f32 / self.canvas_zoom;
        let h = size.height as f32 / self.canvas_zoom;
        let uniforms = Uniforms { projection: ortho_pan(w, h, self.canvas_pan.0, self.canvas_pan.1) };
        self.gpu.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Check if click hits the zoom buttons. Returns true if handled.
    pub fn check_zoom_buttons(&mut self, x: f32, y: f32) -> bool {
        let s = self.scale_factor;
        let size = self.window.inner_size();
        let btn_w = 32.0 * s;
        let btn_h = 32.0 * s;
        let margin = 16.0 * s;
        let gap = 4.0 * s;
        let pill_h = btn_h * 2.0 + gap;
        let total_ui_h = pill_h + gap * 2.0 + btn_h;

        let bx = size.width as f32 - margin - btn_w;
        let by = size.height as f32 - margin - total_ui_h;

        // + button (top half of pill)
        if x >= bx && x < bx + btn_w && y >= by && y < by + btn_h {
            self.zoom_in();
            return true;
        }
        // - button (bottom half of pill)
        let by2 = by + btn_h + gap;
        if x >= bx && x < bx + btn_w && y >= by2 && y < by2 + btn_h {
            self.zoom_out();
            return true;
        }
        // Theme toggle (below the zoom pill)
        let toggle_y = by + pill_h + 4.0 * s * 2.0;
        if x >= bx && x < bx + btn_w && y >= toggle_y && y < toggle_y + btn_h {
            self.toggle_theme();
            return true;
        }
        false
    }

    /// Convert canvas coordinates to grid (row, col) for a given tile.
    pub fn canvas_to_grid(&self, cx: f32, cy: f32, id: usize) -> Option<(usize, usize)> {
        let s = self.scale_factor;
        let bar_h = TITLE_BAR_HEIGHT * s;
        let padding = self.config.appearance.padding as f32 * s;
        let cell_w = self.atlas.cell_width;
        let cell_h = self.atlas.cell_height;

        let tile = self.canvas.tile(id)?;
        let pane = self.panes.get(&id)?;
        let content_y = tile.y + bar_h;

        let is_full_grid = pane.grid.alternate_screen || pane.passthrough;
        let (rel_x, rel_y) = if is_full_grid {
            let ry = cy - content_y - padding;
            let rx = cx - tile.x - padding;
            if ry < 0.0 || rx < 0.0 { return None; }
            (rx, ry)
        } else {
            let input_padding = 8.0 * s;
            let input_bar_h = input_padding * 2.0 + cell_h;
            let input_gap = 6.0 * s;
            let output_area_h = tile.h - input_bar_h - input_gap;
            if cy > content_y + output_area_h { return None; }
            let grid_content_h = padding + pane.grid.rows as f32 * cell_h;
            let output_scroll = if grid_content_h > output_area_h {
                grid_content_h - output_area_h
            } else {
                0.0
            };
            let output_oy = content_y - output_scroll;
            let ry = cy - output_oy - padding;
            let rx = cx - tile.x - padding;
            if ry < 0.0 || rx < 0.0 { return None; }
            (rx, ry)
        };

        let row = (rel_y / cell_h) as usize;
        let col = (rel_x / cell_w) as usize;
        if row >= pane.grid.rows || col >= pane.grid.cols { return None; }
        Some((row, col))
    }

    /// Get the selected text from the terminal grid.
    pub fn selected_text(&self) -> Option<String> {
        let sel = self.selection.as_ref()?;
        let pane = self.panes.get(&sel.tile_id)?;
        let ((sr, sc), (er, ec)) = sel.ordered();
        let mut result = String::new();
        for row in sr..=er {
            if row >= pane.grid.rows { break; }
            let line = pane.grid.display_line(row);
            let col_start = if row == sr { sc } else { 0 };
            let col_end = if row == er { ec + 1 } else { pane.grid.cols };
            let col_end = col_end.min(line.len());
            for col in col_start..col_end {
                result.push(line[col].c);
            }
            if row != er {
                // Trim trailing spaces from each line
                let trimmed = result.trim_end_matches(' ');
                result = trimmed.to_string();
                result.push('\n');
            }
        }
        let trimmed = result.trim_end().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    }

    /// Check if cursor (in physical coords) is over a tile.
    pub fn cursor_over_tile(&self, cursor: (f64, f64)) -> bool {
        let (cx, cy) = self.screen_to_canvas(cursor.0 as f32, cursor.1 as f32);
        self.canvas.hit_test(cx, cy, self.scale_factor).is_some()
    }

    /// Convert screen coords to canvas coords (applying zoom + pan).
    pub fn screen_to_canvas(&self, x: f32, y: f32) -> (f32, f32) {
        (x / self.canvas_zoom + self.canvas_pan.0, y / self.canvas_zoom + self.canvas_pan.1)
    }

    pub fn mouse_down(&mut self, x: f32, y: f32) {
        if self.check_zoom_buttons(x, y) {
            return;
        }
        // Clear selection on any new click
        self.selection = None;
        self.selecting = false;

        let (cx, cy) = self.screen_to_canvas(x, y);
        if let Some((id, in_title, in_resize, in_close)) = self.canvas.hit_test(cx, cy, self.scale_factor) {
            if in_close {
                self.close_tile(id);
                return;
            }
            self.canvas.focus(id);
            if in_resize {
                self.canvas.start_drag(id, DragMode::Resize, cx, cy);
            } else if in_title {
                self.canvas.start_drag(id, DragMode::Move, cx, cy);
            } else {
                // Click on content area: start text selection
                if let Some((row, col)) = self.canvas_to_grid(cx, cy, id) {
                    self.selection = Some(Selection {
                        tile_id: id,
                        start_row: row,
                        start_col: col,
                        end_row: row,
                        end_col: col,
                    });
                    self.selecting = true;
                }
            }
        } else {
            // Click on empty canvas: pan
            self.panning = Some((x, y));
        }
    }

    pub fn middle_mouse_down(&mut self, x: f32, y: f32) {
        self.panning = Some((x, y));
    }

    pub fn mouse_move(&mut self, x: f32, y: f32) {
        if let Some((sx, sy)) = self.panning {
            let dx = (x - sx) / self.canvas_zoom;
            let dy = (y - sy) / self.canvas_zoom;
            self.canvas_pan.0 -= dx;
            self.canvas_pan.1 -= dy;
            self.panning = Some((x, y));
            self.update_projection();
        } else if self.canvas.drag.is_some() {
            let (cx, cy) = self.screen_to_canvas(x, y);
            let resized = self.canvas.update_drag(cx, cy, self.scale_factor);
            if resized {
                if let Some(drag) = &self.canvas.drag {
                    if drag.mode == DragMode::Resize {
                        let id = drag.tile_id;
                        self.resize_pane_to_tile(id);
                    }
                }
            }
        } else if self.selecting {
            if let Some(tile_id) = self.selection.as_ref().map(|s| s.tile_id) {
                let (cx, cy) = self.screen_to_canvas(x, y);
                if let Some((row, col)) = self.canvas_to_grid(cx, cy, tile_id) {
                    if let Some(sel) = &mut self.selection {
                        sel.end_row = row;
                        sel.end_col = col;
                    }
                }
            }
        }
    }

    pub fn mouse_up(&mut self) {
        let had_drag = self.canvas.drag.is_some();
        let had_pan = self.panning.is_some();
        self.panning = None;
        self.selecting = false;
        // Clear selection if it's just a click (no drag distance)
        if let Some(sel) = &self.selection {
            if sel.start_row == sel.end_row && sel.start_col == sel.end_col {
                self.selection = None;
            }
        }
        if let Some(drag) = &self.canvas.drag {
            if drag.mode == DragMode::Resize {
                let id = drag.tile_id;
                self.resize_pane_to_tile(id);
            }
        }
        self.canvas.end_drag();
        if had_drag || had_pan {
            self.mark_dirty();
        }
    }

    pub fn middle_mouse_up(&mut self) {
        if self.panning.is_some() {
            self.mark_dirty();
        }
        self.panning = None;
    }

    pub fn scroll(&mut self, delta: i32) {
        if let Some(id) = self.canvas.focused_id() {
            if let Some(pane) = self.panes.get_mut(&id) {
                pane.grid.scroll_viewport(delta);
            }
        }
    }

    pub fn start_rename(&mut self) {
        if let Some(id) = self.canvas.focused_id() {
            if let Some(tile) = self.canvas.tile(id) {
                self.rename_buffer = tile.name.clone();
                self.renaming = true;
            }
        }
    }

    pub fn handle_key_event(&mut self, event: &winit::event::KeyEvent) -> AppAction {
        use winit::keyboard::{Key, NamedKey};

        if event.state != winit::event::ElementState::Pressed {
            return AppAction::None;
        }

        // Rename mode: capture text input
        if self.renaming {
            match &event.logical_key {
                Key::Named(NamedKey::Enter) => {
                    let name = self.rename_buffer.clone();
                    self.canvas.rename_focused(name);
                    self.renaming = false;
                    self.mark_dirty();
                }
                Key::Named(NamedKey::Escape) => {
                    self.renaming = false;
                }
                Key::Named(NamedKey::Backspace) => {
                    if self.modifiers.super_key() {
                        self.rename_buffer.clear();
                    } else if self.modifiers.alt_key() || self.modifiers.control_key() {
                        // Alt/Ctrl+Backspace: delete word backward
                        let trimmed = self.rename_buffer.trim_end();
                        let word_start = trimmed.rfind(|c: char| c == ' ' || c == '-' || c == '_')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        self.rename_buffer.truncate(word_start);
                    } else {
                        self.rename_buffer.pop();
                    }
                }
                Key::Named(NamedKey::Space) => {
                    self.rename_buffer.push(' ');
                }
                Key::Named(NamedKey::Delete) => {
                    // Delete forward: remove char after cursor (simplified: ignore in rename)
                }
                Key::Character(c) if !self.modifiers.super_key() => {
                    self.rename_buffer.push_str(c.as_str());
                }
                _ => {}
            }
            return AppAction::None;
        }

        if self.modifiers.super_key() {
            if let Key::Character(ref c) = event.logical_key {
                match c.as_str() {
                    "n" => return AppAction::SpawnTile,
                    "w" => return AppAction::ClosePane,
                    "q" => return AppAction::Quit,
                    "=" | "+" | "-" => return AppAction::None,
                    "v" => {
                        // Cmd+V: paste from clipboard (text or image)
                        let content = clipboard_read().or_else(clipboard_read_image);
                        if let Some(text) = content {
                            if let Some(id) = self.canvas.focused_id() {
                                if let Some(pane) = self.panes.get_mut(&id) {
                                    if pane.grid.alternate_screen || pane.passthrough {
                                        let _ = pane.pty.write(text.as_bytes());
                                    } else {
                                        pane.input_insert(&text);
                                    }
                                    pane.cursor_renderer.reset_blink();
                                }
                            }
                        }
                        return AppAction::None;
                    }
                    "c" => {
                        // Cmd+C: copy selection to clipboard, or interrupt if no selection
                        if let Some(text) = self.selected_text() {
                            clipboard_write(&text);
                            self.selection = None;
                            return AppAction::None;
                        }
                        // No selection: fall through to send Ctrl+C / interrupt
                    }
                    _ => {}
                }
            }
        }

        // Alternate screen or passthrough mode: bypass input buffer, send directly to PTY
        let is_passthrough = self.canvas.focused_id()
            .and_then(|id| self.panes.get(&id))
            .map(|p| p.grid.alternate_screen || p.passthrough)
            .unwrap_or(false);

        if is_passthrough {
            if let Some(bytes) = keyboard::key_to_pty_bytes(event, self.modifiers) {
                if let Some(id) = self.canvas.focused_id() {
                    if let Some(pane) = self.panes.get_mut(&id) {
                        let _ = pane.pty.write(&bytes);
                        pane.cursor_renderer.reset_blink();
                    }
                }
            }
            return AppAction::None;
        }

        // Input buffer mode: capture keys into the pane's input buffer
        let alt = self.modifiers.alt_key();
        let ctrl = self.modifiers.control_key();
        let super_key = self.modifiers.super_key();

        if let Some(id) = self.canvas.focused_id() {
            if let Some(pane) = self.panes.get_mut(&id) {
                // Cancel completion on any non-Tab key
                let is_tab = matches!(&event.logical_key, Key::Named(NamedKey::Tab));
                if !is_tab {
                    pane.cancel_completion();
                }

                let shift = self.modifiers.shift_key();

                match &event.logical_key {
                    Key::Named(NamedKey::Enter) => {
                        if shift {
                            // Shift+Enter: insert newline
                            pane.input_insert("\n");
                            pane.ensure_cursor_visible(5);
                            pane.cursor_renderer.reset_blink();
                        } else {
                            let cmd = pane.input_buffer.trim().to_string();
                            pane.submit_input();
                            if !cmd.is_empty() {
                                self.command_history.push(&cmd);
                            }
                            pane.history_index = None;
                            pane.cursor_renderer.reset_blink();
                            pane.grid.scroll_offset = 0;
                        }
                    }
                    Key::Named(NamedKey::Space) => {
                        pane.input_insert(" ");
                        pane.cursor_renderer.reset_blink();
                    }
                    Key::Named(NamedKey::Tab) => {
                        if pane.completion.is_some() {
                            // Cycle through candidates
                            pane.cycle_completion();
                        } else {
                            // Start new completion
                            let cwd = pane.pty.cwd()
                                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                            let state = completion::complete(
                                &pane.input_buffer,
                                pane.input_cursor,
                                &cwd,
                                &self.command_history,
                            );
                            if !state.candidates.is_empty() {
                                pane.completion = Some(state);
                                pane.apply_completion(0);
                            }
                        }
                        pane.cursor_renderer.reset_blink();
                    }
                    Key::Named(NamedKey::Escape) => {
                        // Already cancelled above, but also reset history nav
                        pane.history_index = None;
                    }
                    Key::Named(NamedKey::Delete) => {
                        return AppAction::ClosePane;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        if super_key {
                            pane.input_buffer.drain(..pane.input_cursor);
                            pane.input_cursor = 0;
                        } else if alt || ctrl {
                            pane.input_delete_word_back();
                        } else {
                            pane.input_backspace();
                        }
                        pane.ensure_cursor_visible(5);
                        pane.cursor_renderer.reset_blink();
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        if alt || super_key {
                            pane.input_move_word_left();
                        } else {
                            pane.input_move_left();
                        }
                        pane.cursor_renderer.reset_blink();
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        if alt || super_key {
                            pane.input_move_word_right();
                        } else {
                            pane.input_move_right();
                        }
                        pane.cursor_renderer.reset_blink();
                    }
                    Key::Named(NamedKey::Home) => {
                        pane.input_cursor = 0;
                        pane.cursor_renderer.reset_blink();
                    }
                    Key::Named(NamedKey::End) => {
                        pane.input_cursor = pane.input_buffer.len();
                        pane.cursor_renderer.reset_blink();
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        let next_idx = pane.history_index.map(|i| i + 1).unwrap_or(0);
                        if let Some(entry) = self.command_history.get(next_idx) {
                            if pane.history_index.is_none() {
                                pane.history_stash = pane.input_buffer.clone();
                            }
                            pane.history_index = Some(next_idx);
                            pane.input_buffer = entry.to_string();
                            pane.input_cursor = pane.input_buffer.len();
                            pane.cursor_renderer.reset_blink();
                        }
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        if let Some(idx) = pane.history_index {
                            if idx == 0 {
                                pane.input_buffer = pane.history_stash.clone();
                                pane.input_cursor = pane.input_buffer.len();
                                pane.history_index = None;
                            } else {
                                pane.history_index = Some(idx - 1);
                                if let Some(entry) = self.command_history.get(idx - 1) {
                                    pane.input_buffer = entry.to_string();
                                    pane.input_cursor = pane.input_buffer.len();
                                }
                            }
                            pane.cursor_renderer.reset_blink();
                        }
                    }
                    Key::Character(c) if ctrl => {
                        match c.as_str() {
                            "c" => pane.input_interrupt(),
                            "d" => pane.input_eof(),
                            "l" => {
                                let _ = pane.pty.write(b"\x0c");
                            }
                            "u" => {
                                pane.input_buffer.drain(..pane.input_cursor);
                                pane.input_cursor = 0;
                            }
                            "k" => {
                                pane.input_buffer.truncate(pane.input_cursor);
                            }
                            "w" => pane.input_delete_word_back(),
                            "a" => pane.input_cursor = 0,
                            "e" => pane.input_cursor = pane.input_buffer.len(),
                            _ => {}
                        }
                        pane.cursor_renderer.reset_blink();
                    }
                    Key::Character(c) if !super_key => {
                        pane.input_insert(c.as_str());
                        pane.cursor_renderer.reset_blink();
                        pane.grid.scroll_offset = 0;
                    }
                    _ => {}
                }
            }
        }
        AppAction::None
    }

    pub fn read_all_ptys(&mut self) {
        let mut had_data = false;
        for pane in self.panes.values_mut() {
            let before = pane.grid.dirty;
            pane.read_pty();
            if pane.grid.dirty && !before {
                had_data = true;
            }
        }
        if had_data {
            self.mark_dirty();
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.flush_state_if_needed();
        let s = self.scale_factor;
        let padding = self.config.appearance.padding as f32 * s;
        let bar_h = TITLE_BAR_HEIGHT * s;
        let focused_id = self.canvas.focused_id();
        let draw_order = self.canvas.draw_order();
        let corner_radius = 26.0 * s;

        let canvas_theme = if self.is_dark {
            CanvasTheme::dark()
        } else {
            CanvasTheme::light()
        };

        // Build tile draw batches
        let mut tile_draws: Vec<DrawBatch> = Vec::new();

        for &tile_id in &draw_order {
            let Some(tile) = self.canvas.tile(tile_id) else { continue };
            let tile_clone = crate::ui::canvas::Tile {
                id: tile.id,
                x: tile.x,
                y: tile.y,
                w: tile.w,
                h: tile.h,
                name: tile.name.clone(),
            };
            let Some(pane) = self.panes.get_mut(&tile_id) else { continue };
            let is_focused = Some(tile_id) == focused_id;
            let is_renaming = self.renaming && is_focused;

            let tile_selection = self.selection.as_ref().filter(|sel| sel.tile_id == tile_id);
            let batch = tile_renderer::build_tile_batch(
                &tile_clone,
                pane,
                &mut self.atlas,
                &self.theme,
                &canvas_theme,
                &self.gpu.queue,
                s,
                bar_h,
                padding,
                corner_radius,
                is_focused,
                is_renaming,
                &self.rename_buffer,
                &self.config.terminal.cursor_style,
                tile_selection,
            );
            tile_draws.push(batch);
        }

        // Build dot grid
        let view_w = self.gpu.surface_config.width as f32 / self.canvas_zoom;
        let view_h = self.gpu.surface_config.height as f32 / self.canvas_zoom;
        let (dot_v, dot_i) = grid_renderer::build_dot_grid(
            self.canvas_pan,
            view_w,
            view_h,
            s,
            &canvas_theme,
        );

        // Build UI batch
        ui_renderer::update_stats();
        let tile_count = self.canvas.tiles.len();
        let ui_batch = ui_renderer::build_ui_batch(
            self.canvas_zoom,
            self.canvas_pan,
            self.is_dark,
            &mut self.atlas,
            &canvas_theme,
            &self.gpu.queue,
            self.gpu.surface_config.width as f32,
            self.gpu.surface_config.height as f32,
            s,
            tile_count,
        );

        // GPU draw
        let output = self.gpu.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(canvas_theme.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            // Draw dot grid on canvas background (circular dots)
            if !dot_i.is_empty() {
                let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&dot_v), usage: wgpu::BufferUsages::VERTEX });
                let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&dot_i), usage: wgpu::BufferUsages::INDEX });
                pass.set_pipeline(&self.rounded_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_bind_group(1, &self.texture_bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..dot_i.len() as u32, 0, 0..1);
            }

            for batch in &tile_draws {
                // Rounded rects (border + tile bg + title bar)
                if !batch.rounded_indices.is_empty() {
                    let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&batch.rounded_verts), usage: wgpu::BufferUsages::VERTEX });
                    let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&batch.rounded_indices), usage: wgpu::BufferUsages::INDEX });
                    pass.set_pipeline(&self.rounded_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.texture_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..batch.rounded_indices.len() as u32, 0, 0..1);
                }
                // Regular bg quads (separator, cell bgs, cursor)
                if !batch.bg_indices.is_empty() {
                    let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&batch.bg_verts), usage: wgpu::BufferUsages::VERTEX });
                    let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&batch.bg_indices), usage: wgpu::BufferUsages::INDEX });
                    pass.set_pipeline(&self.bg_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.texture_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..batch.bg_indices.len() as u32, 0, 0..1);
                }
                if !batch.fg_indices.is_empty() {
                    let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&batch.fg_verts), usage: wgpu::BufferUsages::VERTEX });
                    let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&batch.fg_indices), usage: wgpu::BufferUsages::INDEX });
                    pass.set_pipeline(&self.fg_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.texture_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..batch.fg_indices.len() as u32, 0, 0..1);
                }
            }

            // Draw UI (zoom buttons + theme toggle)
            if !ui_batch.rounded_indices.is_empty() {
                let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_batch.rounded_verts), usage: wgpu::BufferUsages::VERTEX });
                let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_batch.rounded_indices), usage: wgpu::BufferUsages::INDEX });
                pass.set_pipeline(&self.rounded_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_bind_group(1, &self.texture_bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..ui_batch.rounded_indices.len() as u32, 0, 0..1);
            }
            if !ui_batch.bg_indices.is_empty() {
                let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_batch.bg_verts), usage: wgpu::BufferUsages::VERTEX });
                let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_batch.bg_indices), usage: wgpu::BufferUsages::INDEX });
                pass.set_pipeline(&self.bg_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_bind_group(1, &self.texture_bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..ui_batch.bg_indices.len() as u32, 0, 0..1);
            }
            if !ui_batch.fg_indices.is_empty() {
                let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_batch.fg_verts), usage: wgpu::BufferUsages::VERTEX });
                let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_batch.fg_indices), usage: wgpu::BufferUsages::INDEX });
                pass.set_pipeline(&self.fg_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_bind_group(1, &self.texture_bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..ui_batch.fg_indices.len() as u32, 0, 0..1);
            }
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    pub fn mark_dirty(&mut self) {
        self.state_dirty = true;
    }

    /// Flush state to disk if dirty and at least 2 seconds since last save.
    pub fn flush_state_if_needed(&mut self) {
        if self.state_dirty && self.last_save.elapsed().as_secs() >= 2 {
            self.save_state();
            self.state_dirty = false;
            self.last_save = Instant::now();
        }
    }

    pub fn save_state(&self) {
        let tiles: Vec<TileState> = self.canvas.tiles.iter().map(|t| {
            TileState {
                x: t.x, y: t.y, w: t.w, h: t.h, name: t.name.clone(),
            }
        }).collect();
        let state = AppState {
            canvas_zoom: self.canvas_zoom,
            canvas_pan: self.canvas_pan,
            is_dark: self.is_dark,
            tiles,
        };
        state.save();
        self.command_history.save();
    }

    /// Given a canvas-space click position, find the URL under the cursor (if any).
    pub fn url_at_canvas_pos(&self, cx: f32, cy: f32) -> Option<String> {
        let s = self.scale_factor;
        let bar_h = TITLE_BAR_HEIGHT * s;
        let padding = self.config.appearance.padding as f32 * s;
        let cell_w = self.atlas.cell_width;
        let cell_h = self.atlas.cell_height;

        // Find which tile was hit
        let (id, in_title, _, _) = self.canvas.hit_test(cx, cy, s)?;
        if in_title { return None; }

        let tile = self.canvas.tile(id)?;
        let pane = self.panes.get(&id)?;

        let content_y = tile.y + bar_h;

        // Determine the grid row/col from canvas position
        let (grid_row, col);

        let is_full_grid = pane.grid.alternate_screen || pane.passthrough;
        if is_full_grid {
            let rel_y = cy - content_y - padding;
            let rel_x = cx - tile.x - padding;
            if rel_y < 0.0 || rel_x < 0.0 { return None; }
            grid_row = (rel_y / cell_h) as usize;
            col = (rel_x / cell_w) as usize;
        } else {
            // Chat mode: output is bottom-anchored and clipped
            let input_padding = 8.0 * s;
            let input_bar_h = input_padding * 2.0 + cell_h;
            let input_gap = 6.0 * s;
            let output_area_h = tile.h - input_bar_h - input_gap;
            let grid_content_h = padding + pane.grid.rows as f32 * cell_h;
            let output_scroll = if grid_content_h > output_area_h {
                grid_content_h - output_area_h
            } else {
                0.0
            };
            let output_oy = content_y - output_scroll;
            let rel_y = cy - output_oy - padding;
            let rel_x = cx - tile.x - padding;
            if rel_y < 0.0 || rel_x < 0.0 { return None; }
            // Check we're in the output area, not the input bar
            if cy > content_y + output_area_h { return None; }
            grid_row = (rel_y / cell_h) as usize;
            col = (rel_x / cell_w) as usize;
        }

        if grid_row >= pane.grid.rows || col >= pane.grid.cols { return None; }

        // Extract the text of the display line
        let line = pane.grid.display_line(grid_row);
        let line_text: String = line.iter().map(|c| c.c).collect();
        let line_text = line_text.trim_end();

        // Find URLs in the line and check if col falls within one
        find_url_at_col(line_text, col)
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }
}

/// Set macOS window titlebar appearance to match theme.
#[cfg(target_os = "macos")]
pub fn set_macos_appearance(window: &Window, dark: bool) {
    use winit::raw_window_handle::HasWindowHandle;
    if let Ok(handle) = window.window_handle() {
        if let winit::raw_window_handle::RawWindowHandle::AppKit(appkit) = handle.as_raw() {
            #[allow(deprecated, unexpected_cfgs)]
            unsafe {
                use cocoa::foundation::NSString as NSStringTrait;
                use objc::runtime::Object;
                use objc::{msg_send, sel, sel_impl, class};
                let ns_view: *mut Object = appkit.ns_view.as_ptr() as *mut Object;
                let ns_window: *mut Object = msg_send![ns_view, window];
                let name_str = if dark {
                    "NSAppearanceNameVibrantDark"
                } else {
                    "NSAppearanceNameVibrantLight"
                };
                let name = cocoa::foundation::NSString::alloc(cocoa::base::nil)
                    .init_str(name_str);
                let appearance: *mut Object = msg_send![
                    class!(NSAppearance),
                    appearanceNamed: name
                ];
                let _: () = msg_send![ns_window, setAppearance: appearance];
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_macos_appearance(_window: &Window, _dark: bool) {}

/// Read text from the macOS clipboard via pbpaste.
fn clipboard_read() -> Option<String> {
    std::process::Command::new("pbpaste")
        .output()
        .ok()
        .and_then(|o| if o.status.success() { String::from_utf8(o.stdout).ok() } else { None })
        .filter(|s| !s.is_empty())
}

/// Write text to the macOS clipboard via pbcopy.
fn clipboard_write(text: &str) {
    use std::io::Write;
    if let Ok(mut child) = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

/// Try to read an image from the macOS clipboard and save it to a temp file.
/// Returns the file path if an image was found.
fn clipboard_read_image() -> Option<String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = format!("/tmp/sunnyterm_paste_{timestamp}.png");
    let script = format!(
        r#"try
    set imgData to the clipboard as «class PNGf»
    set f to open for access POSIX file "{path}" with write permission
    write imgData to f
    close access f
    return "{path}"
on error
    return ""
end try"#
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .ok()?;
    let result = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if result.is_empty() { None } else { Some(result) }
}

/// Find a URL in `text` that spans over column `col`.
fn find_url_at_col(text: &str, col: usize) -> Option<String> {
    let prefixes = ["https://", "http://", "file://"];
    let mut search_from = 0;
    while search_from < text.len() {
        // Find the earliest URL prefix
        let mut earliest: Option<(usize, &str)> = None;
        for pfx in &prefixes {
            if let Some(pos) = text[search_from..].find(pfx) {
                let abs_pos = search_from + pos;
                if earliest.is_none() || abs_pos < earliest.unwrap().0 {
                    earliest = Some((abs_pos, pfx));
                }
            }
        }
        let (start, _) = earliest?;

        // Extend to the end of the URL (stop at whitespace or common delimiters)
        let end = text[start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '>' | '<' | '"' | '\'' | ')' | ']'))
            .map(|i| start + i)
            .unwrap_or(text.len());

        // Check if col falls within this URL (using char positions)
        let char_start = text[..start].chars().count();
        let char_end = text[..end].chars().count();

        if col >= char_start && col < char_end {
            return Some(text[start..end].to_string());
        }

        search_from = end;
    }
    None
}
