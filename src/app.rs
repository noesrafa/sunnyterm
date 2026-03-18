use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use winit::keyboard::ModifiersState;
use winit::window::Window;

use crate::config::Config;
use crate::input::keyboard;
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

    bg_pipeline: wgpu::RenderPipeline,
    fg_pipeline: wgpu::RenderPipeline,
    rounded_pipeline: wgpu::RenderPipeline,
    uniform_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    texture_bind_group: wgpu::BindGroup,

    window: Arc<Window>,

    state_dirty: bool,
    last_save: Instant,
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
                let mut pane = Pane::new(&config.terminal.shell, cols, rows, config.terminal.cursor_blink);

                // Restore previous session: push saved scrollback + visible
                // grid into scrollback so old content appears above the new shell
                {
                    use crate::terminal::cell::Cell;

                    let mut history = ts.scrollback;
                    // Add non-empty grid lines to history
                    let mut last_non_empty = 0;
                    for (i, row) in ts.grid_cells.iter().enumerate() {
                        if row.iter().any(|c| c.c != ' ') {
                            last_non_empty = i + 1;
                        }
                    }
                    history.extend(ts.grid_cells.into_iter().take(last_non_empty));

                    if !history.is_empty() {
                        // Add a separator line between old session and new
                        let sep_cols = pane.grid.cols;
                        let sep_line: Vec<Cell> = (0..sep_cols)
                            .map(|i| Cell { c: if i < sep_cols { '─' } else { ' ' }, ..Cell::default() })
                            .collect();
                        history.push(sep_line);

                        pane.grid.scrollback.restore(history);
                        pane.grid.dirty = true;
                    }
                }

                panes.insert(tile_id, pane);
            }
            initial_zoom = saved.canvas_zoom;
            initial_pan_x = saved.canvas_pan.0;
            initial_pan_y = saved.canvas_pan.1;
            is_dark_init = saved.is_dark;
        } else {
            // First launch: create default tile
            let tw = 800.0 * scale_factor;
            let th = 800.0 * scale_factor;
            let tx = (size.width as f32 - tw) / 2.0;
            let ty = (size.height as f32 - th - bar_h) / 2.0;
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
            bg_pipeline, fg_pipeline, rounded_pipeline,
            uniform_bind_group, uniform_buffer, texture_bind_group,
            window,
            state_dirty: false,
            last_save: Instant::now(),
        }
    }

    fn default_tile_size(&self) -> (f32, f32) {
        let s = self.scale_factor;
        (800.0 * s, 800.0 * s)
    }

    pub fn spawn_tile(&mut self) {
        let size = self.window.inner_size();
        let s = self.scale_factor;
        let (tw, th) = self.default_tile_size();
        let offset = (self.canvas.tiles.len() as f32 * 30.0 * s) % (200.0 * s);
        let x = ((size.width as f32 - tw) / 2.0 + offset).max(0.0);
        let y = ((size.height as f32 - th) / 2.0 + offset).max(0.0);

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
        let x = cx - tw / 2.0;
        let y = cy - th / 2.0;

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

    fn update_projection(&mut self) {
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

    /// Check if cursor (in logical coords) is over a tile.
    pub fn cursor_over_tile(&self, cursor: (f64, f64)) -> bool {
        let s = self.scale_factor;
        let (cx, cy) = self.screen_to_canvas(cursor.0 as f32 * s, cursor.1 as f32 * s);
        self.canvas.hit_test(cx, cy, s).is_some()
    }

    /// Convert screen coords to canvas coords (applying zoom + pan).
    pub fn screen_to_canvas(&self, x: f32, y: f32) -> (f32, f32) {
        (x / self.canvas_zoom + self.canvas_pan.0, y / self.canvas_zoom + self.canvas_pan.1)
    }

    pub fn mouse_down(&mut self, x: f32, y: f32) {
        if self.check_zoom_buttons(x, y) {
            return;
        }
        let (cx, cy) = self.screen_to_canvas(x, y);
        if let Some((id, _in_title, in_resize, in_close)) = self.canvas.hit_test(cx, cy, self.scale_factor) {
            if in_close {
                self.close_tile(id);
                return;
            }
            self.canvas.focus(id);
            if in_resize {
                self.canvas.start_drag(id, DragMode::Resize, cx, cy);
            } else {
                self.canvas.start_drag(id, DragMode::Move, cx, cy);
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
        }
    }

    pub fn mouse_up(&mut self) {
        let had_drag = self.canvas.drag.is_some();
        let had_pan = self.panning.is_some();
        self.panning = None;
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
                    _ => {}
                }
            }
        }

        // Alternate screen mode: bypass input buffer, send directly to PTY
        let is_alternate = self.canvas.focused_id()
            .and_then(|id| self.panes.get(&id))
            .map(|p| p.grid.alternate_screen)
            .unwrap_or(false);

        if is_alternate {
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
                match &event.logical_key {
                    Key::Named(NamedKey::Enter) => {
                        pane.submit_input();
                        pane.cursor_renderer.reset_blink();
                        pane.grid.scroll_offset = 0;
                    }
                    Key::Named(NamedKey::Space) => {
                        pane.input_insert(" ");
                        pane.cursor_renderer.reset_blink();
                    }
                    Key::Named(NamedKey::Tab) => {
                        pane.input_insert("    ");
                        pane.cursor_renderer.reset_blink();
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
                    Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown) => {
                        // TODO: command history
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
        let corner_radius = 10.0 * s;

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
            let (scrollback, grid_cells, cursor_row, cursor_col) =
                if let Some(pane) = self.panes.get(&t.id) {
                    let sb = pane.grid.scrollback.lines();
                    // Keep last 1000 scrollback lines to limit state file size
                    let sb_start = sb.len().saturating_sub(1000);
                    (
                        sb[sb_start..].to_vec(),
                        pane.grid.cells.clone(),
                        pane.grid.cursor_row,
                        pane.grid.cursor_col,
                    )
                } else {
                    (Vec::new(), Vec::new(), 0, 0)
                };
            TileState {
                x: t.x, y: t.y, w: t.w, h: t.h, name: t.name.clone(),
                scrollback, grid_cells, cursor_row, cursor_col,
            }
        }).collect();
        let state = AppState {
            canvas_zoom: self.canvas_zoom,
            canvas_pan: self.canvas_pan,
            is_dark: self.is_dark,
            tiles,
        };
        state.save();
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }
}
