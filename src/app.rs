use std::collections::HashMap;
use std::sync::Arc;

use winit::keyboard::ModifiersState;
use winit::window::Window;

use crate::config::Config;
use crate::input::keyboard;
use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::cursor::CursorRenderer;
use crate::renderer::gpu::GpuContext;
use crate::renderer::text::{TextRenderer, TextVertex};
use crate::terminal::grid::Grid;
use crate::terminal::parser::TermParser;
use crate::terminal::pty::Pty;
use crate::ui::canvas::{Canvas, DragMode, TITLE_BAR_HEIGHT};
use crate::ui::theme::{Color, Theme};

use wgpu::util::DeviceExt;

struct Pane {
    grid: Grid,
    parser: TermParser,
    pty: Pty,
    text_renderer: TextRenderer,
    cursor_renderer: CursorRenderer,
}

impl Pane {
    fn new(shell: &str, cols: usize, rows: usize, cursor_blink: bool) -> Self {
        Self {
            grid: Grid::new(cols, rows),
            parser: TermParser::new(),
            pty: Pty::spawn(shell, cols as u16, rows as u16).expect("Failed to spawn PTY"),
            text_renderer: TextRenderer::new(),
            cursor_renderer: CursorRenderer::new(cursor_blink),
        }
    }

    fn read_pty(&mut self) {
        let data = self.pty.try_read();
        if !data.is_empty() {
            self.parser.process(&data, &mut self.grid);
        }
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        if cols > 0 && rows > 0 && (cols != self.grid.cols || rows != self.grid.rows) {
            self.grid.resize(cols, rows);
            let _ = self.pty.resize(cols as u16, rows as u16);
        }
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

    bg_pipeline: wgpu::RenderPipeline,
    fg_pipeline: wgpu::RenderPipeline,
    rounded_pipeline: wgpu::RenderPipeline,
    uniform_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    texture_bind_group: wgpu::BindGroup,

    window: Arc<Window>,
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
        let theme = Theme::catppuccin_mocha();

        let size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;

        let atlas = GlyphAtlas::new(
            &gpu.device,
            &gpu.queue,
            config.appearance.font_size,
            scale_factor,
        );

        // Create first tile at full window size, centered
        let mut canvas = Canvas::new();
        let bar_h = TITLE_BAR_HEIGHT * scale_factor;
        let tw = 800.0 * scale_factor;
        let th = 800.0 * scale_factor;
        let tx = (size.width as f32 - tw) / 2.0;
        let ty = (size.height as f32 - th - bar_h) / 2.0;
        let tile_id = canvas.spawn(tx, ty, tw, th);

        let padding = config.appearance.padding as f32 * scale_factor;
        let cols = ((tw - padding * 2.0) / atlas.cell_width).max(1.0) as usize;
        let rows = ((th - bar_h - padding * 2.0) / atlas.cell_height).max(1.0) as usize;

        let pane = Pane::new(&config.terminal.shell, cols, rows, config.terminal.cursor_blink);
        let mut panes = HashMap::new();
        panes.insert(tile_id, pane);

        // Shaders & pipelines
        let text_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../assets/shaders/text.wgsl").into()),
        });

        let initial_zoom = 1.0;
        // Center the tile in the viewport: pan so the tile center maps to screen center
        let view_w = size.width as f32 / initial_zoom;
        let view_h = size.height as f32 / initial_zoom;
        let tile_center_x = tx + tw / 2.0;
        let tile_center_y = ty + (th + bar_h) / 2.0;
        let initial_pan_x = tile_center_x - view_w / 2.0;
        let initial_pan_y = tile_center_y - view_h / 2.0;
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
            is_dark: true,
            bg_pipeline, fg_pipeline, rounded_pipeline,
            uniform_bind_group, uniform_buffer, texture_bind_group,
            window,
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
    }

    pub fn close_focused(&mut self) {
        if self.panes.len() <= 1 { return; }
        if let Some(id) = self.canvas.focused_id() {
            self.canvas.remove(id);
            self.panes.remove(&id);
        }
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
        if let Some((id, _in_title, in_resize)) = self.canvas.hit_test(cx, cy, self.scale_factor) {
            self.canvas.focus(id);
            if in_resize {
                self.canvas.start_drag(id, DragMode::Resize, cx, cy);
            } else {
                // Any click on tile (title bar or content): move the tile
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
        self.panning = None;
        if let Some(drag) = &self.canvas.drag {
            if drag.mode == DragMode::Resize {
                let id = drag.tile_id;
                self.resize_pane_to_tile(id);
            }
        }
        self.canvas.end_drag();
    }

    pub fn middle_mouse_up(&mut self) {
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
                }
                Key::Named(NamedKey::Escape) => {
                    self.renaming = false;
                }
                Key::Named(NamedKey::Backspace) => {
                    self.rename_buffer.pop();
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

        if let Some(bytes) = keyboard::key_to_pty_bytes(event, self.modifiers) {
            if let Some(id) = self.canvas.focused_id() {
                if let Some(pane) = self.panes.get_mut(&id) {
                    let _ = pane.pty.write(&bytes);
                    pane.cursor_renderer.reset_blink();
                }
            }
        }
        AppAction::None
    }

    pub fn read_all_ptys(&mut self) {
        for pane in self.panes.values_mut() {
            pane.read_pty();
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let s = self.scale_factor;
        let padding = self.config.appearance.padding as f32 * s;
        let bar_h = TITLE_BAR_HEIGHT * s;
        let focused_id = self.canvas.focused_id();
        let draw_order = self.canvas.draw_order();
        let dark = self.is_dark;

        // Canvas colors based on theme
        let canvas_clear = if dark {
            wgpu::Color { r: 0.0024, g: 0.0024, b: 0.0024, a: 1.0 }
        } else {
            wgpu::Color { r: 0.5647, g: 0.5457, b: 0.4452, a: 1.0 }
        };
        let tile_bar_color = if dark { Color::from_hex(0x1B1D1F) } else { Color::from_hex(0xF5F5F5) };
        let tile_border = if dark { Color::from_hex(0x353B40) } else { Color::from_hex(0xD6CEC4) };
        let title_focused = if dark { Color::from_hex(0x888888) } else { Color::from_hex(0x444444) };
        let title_unfocused = if dark { Color::from_hex(0x555555) } else { Color::from_hex(0x999999) };
        let ui_btn_bg = if dark { Color::from_hex(0x1B1D1F) } else { Color::from_hex(0xF5F5F5) };
        let ui_btn_border = if dark { Color::from_hex(0x353B40) } else { Color::from_hex(0xD6CEC4) };
        let ui_icon = if dark { Color::from_hex(0x888888) } else { Color::from_hex(0x555555) };
        let ui_label = if dark { Color::from_hex(0x555555) } else { Color::from_hex(0x888888) };
        let dot_dim_a: f32 = if dark { 0.15 } else { 0.25 };
        let dot_bright_a: f32 = if dark { 0.35 } else { 0.50 };
        let dot_rgb: f32 = if dark { 1.0 } else { 0.0 };

        // Per-tile: (rounded_v, rounded_i, bg_v, bg_i, fg_v, fg_i)
        type TileDraw = (Vec<TextVertex>, Vec<u32>, Vec<TextVertex>, Vec<u32>, Vec<TextVertex>, Vec<u32>);
        let mut tile_draws: Vec<TileDraw> = Vec::new();
        let corner_radius = 10.0 * s;

        for &tile_id in &draw_order {
            let Some(tile) = self.canvas.tile(tile_id) else { continue };
            let tile_name = tile.name.clone();
            let Some(pane) = self.panes.get_mut(&tile_id) else { continue };
            let is_focused = Some(tile_id) == focused_id;

            let tx = tile.x;
            let ty = tile.y;
            let tw = tile.w;
            let th = tile.h;
            let total_h = th + bar_h;

            let mut rnd_v: Vec<TextVertex> = Vec::new();
            let mut rnd_i: Vec<u32> = Vec::new();
            let mut bg_v: Vec<TextVertex> = Vec::new();
            let mut bg_i: Vec<u32> = Vec::new();
            let mut fg_v: Vec<TextVertex> = Vec::new();
            let mut fg_i: Vec<u32> = Vec::new();

            let border_color = tile_border.to_array();
            let bw = s;
            let bw2 = bw * 2.0;
            let br = corner_radius + bw;

            // 1) Border (larger rounded rect, drawn first = behind)
            push_rounded_quad(&mut rnd_v, &mut rnd_i,
                tx - bw, ty - bw, tw + bw2, total_h + bw2, tw + bw2, total_h + bw2, br, border_color);

            // 2) Tile background (rounded)
            let tile_bg = self.theme.background.to_array();
            push_rounded_quad(&mut rnd_v, &mut rnd_i,
                tx, ty, tw, total_h, tw, total_h, corner_radius, tile_bg);

            // 3) Title bar
            let bar_color = tile_bar_color.to_array();
            push_rounded_quad(&mut rnd_v, &mut rnd_i,
                tx, ty, tw, bar_h, tw, total_h, corner_radius, bar_color);

            let content_y = ty + bar_h;
            // Separator line
            push_quad(&mut bg_v, &mut bg_i, tx + 1.0, content_y, tw - 2.0, bw * 0.5, border_color);

            // Resize handle indicator (small triangle-ish in bottom-right corner)
            let handle_size = 8.0 * s;
            let handle_color = border_color;
            // Two small lines to suggest a grip
            push_quad(&mut bg_v, &mut bg_i,
                tx + tw - handle_size - 2.0 * s, ty + total_h - 3.0 * s,
                handle_size, bw, handle_color);
            push_quad(&mut bg_v, &mut bg_i,
                tx + tw - 3.0 * s, ty + total_h - handle_size - 2.0 * s,
                bw, handle_size, handle_color);

            // Title text
            let is_renaming = self.renaming && is_focused;
            let display_name = if is_renaming {
                format!("{}|", &self.rename_buffer)
            } else {
                tile_name
            };
            let title_color = if is_renaming {
                self.theme.foreground.to_array()
            } else if is_focused {
                title_focused.to_array()
            } else {
                title_unfocused.to_array()
            };
            let title_y = ty + (bar_h - self.atlas.cell_height) / 2.0;
            let mut title_x = tx + 10.0 * s;
            for c in display_name.chars() {
                if title_x + self.atlas.cell_width > tx + tw - 10.0 * s {
                    break;
                }
                if c != ' ' {
                    let glyph = self.atlas.get_or_rasterize(c, false, false, &self.gpu.queue);
                    if glyph.width > 0.0 && glyph.height > 0.0 {
                        let gx = title_x + glyph.bearing_x;
                        let gy = title_y + (self.atlas.cell_height - glyph.bearing_y);
                        let fg_base = fg_v.len() as u32;
                        fg_v.extend_from_slice(&[
                            TextVertex { position: [gx, gy], tex_coords: [glyph.tex_x, glyph.tex_y], color: title_color, bg_color: [0.0; 4] },
                            TextVertex { position: [gx + glyph.width, gy], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y], color: title_color, bg_color: [0.0; 4] },
                            TextVertex { position: [gx + glyph.width, gy + glyph.height], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y + glyph.tex_h], color: title_color, bg_color: [0.0; 4] },
                            TextVertex { position: [gx, gy + glyph.height], tex_coords: [glyph.tex_x, glyph.tex_y + glyph.tex_h], color: title_color, bg_color: [0.0; 4] },
                        ]);
                        fg_i.extend_from_slice(&[fg_base, fg_base+1, fg_base+2, fg_base, fg_base+2, fg_base+3]);
                    }
                }
                title_x += self.atlas.cell_width;
            }

            // Pane text
            pane.cursor_renderer.visible = is_focused;
            pane.cursor_renderer.update();
            pane.text_renderer.build_vertices(
                &pane.grid, &mut self.atlas, &self.theme, padding, &self.gpu.queue,
            );

            let ox = tx;
            let oy = content_y;

            let bg_base = bg_v.len() as u32;
            for v in &pane.text_renderer.bg_vertices {
                bg_v.push(TextVertex { position: [v.position[0] + ox, v.position[1] + oy], ..*v });
            }
            for idx in &pane.text_renderer.bg_indices { bg_i.push(idx + bg_base); }

            let fg_base = fg_v.len() as u32;
            for v in &pane.text_renderer.fg_vertices {
                fg_v.push(TextVertex { position: [v.position[0] + ox, v.position[1] + oy], ..*v });
            }
            for idx in &pane.text_renderer.fg_indices { fg_i.push(idx + fg_base); }

            if is_focused {
                let (cverts, cidxs) = pane.cursor_renderer.build_vertices(
                    pane.grid.cursor_row, pane.grid.cursor_col,
                    self.atlas.cell_width, self.atlas.cell_height,
                    padding, &self.config.terminal.cursor_style, &self.theme,
                );
                let cur_base = bg_v.len() as u32;
                for v in &cverts { bg_v.push(TextVertex { position: [v.position[0] + ox, v.position[1] + oy], ..*v }); }
                for idx in &cidxs { bg_i.push(idx + cur_base); }
            }

            tile_draws.push((rnd_v, rnd_i, bg_v, bg_i, fg_v, fg_i));
        }

        // Build dot grid for the canvas background (using rounded pipeline for circles)
        let mut dot_v: Vec<TextVertex> = Vec::new();
        let mut dot_i: Vec<u32> = Vec::new();
        {
            let pan = self.canvas_pan;
            let view_w = self.gpu.surface_config.width as f32 / self.canvas_zoom;
            let view_h = self.gpu.surface_config.height as f32 / self.canvas_zoom;
            let dot_spacing = 24.0 * s;
            let dot_small = 2.0 * s;
            let dot_large = 3.2 * s;
            let color_dim = [dot_rgb, dot_rgb, dot_rgb, dot_dim_a];
            let color_bright = [dot_rgb, dot_rgb, dot_rgb, dot_bright_a];
            let major = 6; // every 6th dot is brighter
            let start_x = (pan.0 / dot_spacing).floor() * dot_spacing;
            let start_y = (pan.1 / dot_spacing).floor() * dot_spacing;
            let mut gx = start_x;
            let mut ix: i32 = ((start_x / dot_spacing).round()) as i32;
            while gx < pan.0 + view_w + dot_spacing {
                let mut gy = start_y;
                let mut iy: i32 = ((start_y / dot_spacing).round()) as i32;
                while gy < pan.1 + view_h + dot_spacing {
                    let is_major = ix.rem_euclid(major) == 0 && iy.rem_euclid(major) == 0;
                    let (sz, col) = if is_major { (dot_large, color_bright) } else { (dot_small, color_dim) };
                    push_rounded_quad(&mut dot_v, &mut dot_i, gx - sz * 0.5, gy - sz * 0.5, sz, sz, sz, sz, sz * 0.5, col);
                    gy += dot_spacing;
                    iy += 1;
                }
                gx += dot_spacing;
                ix += 1;
            }
        }

        // GPU draw: render each tile fully (bg then fg) before the next
        let output = self.gpu.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(canvas_clear),
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

            for (rnd_v, rnd_i, bg_v, bg_i, fg_v, fg_i) in &tile_draws {
                // Rounded rects (border + tile bg + title bar)
                if !rnd_i.is_empty() {
                    let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(rnd_v), usage: wgpu::BufferUsages::VERTEX });
                    let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(rnd_i), usage: wgpu::BufferUsages::INDEX });
                    pass.set_pipeline(&self.rounded_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.texture_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..rnd_i.len() as u32, 0, 0..1);
                }
                // Regular bg quads (separator, cell bgs, cursor)
                if !bg_i.is_empty() {
                    let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(bg_v), usage: wgpu::BufferUsages::VERTEX });
                    let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(bg_i), usage: wgpu::BufferUsages::INDEX });
                    pass.set_pipeline(&self.bg_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.texture_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..bg_i.len() as u32, 0, 0..1);
                }
                if !fg_i.is_empty() {
                    let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(fg_v), usage: wgpu::BufferUsages::VERTEX });
                    let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(fg_i), usage: wgpu::BufferUsages::INDEX });
                    pass.set_pipeline(&self.fg_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.texture_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..fg_i.len() as u32, 0, 0..1);
                }
            }

            // Zoom UI (bottom-right, fixed on screen)
            {
                let zoom = self.canvas_zoom;
                let pan = self.canvas_pan;
                let sw = self.gpu.surface_config.width as f32 / zoom + pan.0;
                let sh = self.gpu.surface_config.height as f32 / zoom + pan.1;
                let z = 1.0 / zoom; // scale factor for UI elements
                let btn_w = 32.0 * s * z;
                let btn_h = 32.0 * s * z;
                let margin = 16.0 * s * z;
                let gap = 4.0 * s * z;
                let radius = 8.0 * s * z;
                let bw = 1.0 * s * z;

                let mut ui_rnd: Vec<TextVertex> = Vec::new();
                let mut ui_rnd_i: Vec<u32> = Vec::new();
                let mut ui_bg: Vec<TextVertex> = Vec::new();
                let mut ui_bg_i: Vec<u32> = Vec::new();
                let mut ui_fg: Vec<TextVertex> = Vec::new();
                let mut ui_fg_i: Vec<u32> = Vec::new();

                let btn_bg = ui_btn_bg.to_array();
                let btn_border = ui_btn_border.to_array();
                let icon_color = ui_icon.to_array();
                let line_w = 1.5 * s * z;

                // Container: both buttons in one rounded pill + toggle below
                let pill_h = btn_h * 2.0 + gap;
                let total_ui_h = pill_h + gap * 2.0 + btn_h; // pill + gap + toggle
                let bx = sw - margin - btn_w;
                let by = sh - margin - total_ui_h;

                // Border (outer rounded rect)
                push_rounded_quad(&mut ui_rnd, &mut ui_rnd_i,
                    bx - bw, by - bw, btn_w + bw * 2.0, pill_h + bw * 2.0,
                    btn_w + bw * 2.0, pill_h + bw * 2.0, radius + bw, btn_border);
                // Background (inner rounded rect)
                push_rounded_quad(&mut ui_rnd, &mut ui_rnd_i,
                    bx, by, btn_w, pill_h, btn_w, pill_h, radius, btn_bg);

                // Divider line between buttons
                let div_y = by + btn_h + gap * 0.5 - bw * 0.5;
                push_quad(&mut ui_bg, &mut ui_bg_i, bx + 6.0 * s * z, div_y, btn_w - 12.0 * s * z, bw, btn_border);

                // + icon (top button)
                let icon_len = 10.0 * s * z;
                // horizontal
                push_quad(&mut ui_bg, &mut ui_bg_i,
                    bx + (btn_w - icon_len) / 2.0, by + (btn_h - line_w) / 2.0,
                    icon_len, line_w, icon_color);
                // vertical
                push_quad(&mut ui_bg, &mut ui_bg_i,
                    bx + (btn_w - line_w) / 2.0, by + (btn_h - icon_len) / 2.0,
                    line_w, icon_len, icon_color);

                // - icon (bottom button)
                let by2 = by + btn_h + gap;
                push_quad(&mut ui_bg, &mut ui_bg_i,
                    bx + (btn_w - icon_len) / 2.0, by2 + (btn_h - line_w) / 2.0,
                    icon_len, line_w, icon_color);

                // Theme toggle button (below zoom pill)
                let toggle_y = by + pill_h + gap * 2.0;
                // Border
                push_rounded_quad(&mut ui_rnd, &mut ui_rnd_i,
                    bx - bw, toggle_y - bw, btn_w + bw * 2.0, btn_h + bw * 2.0,
                    btn_w + bw * 2.0, btn_h + bw * 2.0, radius + bw, btn_border);
                // Background
                push_rounded_quad(&mut ui_rnd, &mut ui_rnd_i,
                    bx, toggle_y, btn_w, btn_h, btn_w, btn_h, radius, btn_bg);
                // Sun/Moon icon: circle in center
                let icon_r = 5.0 * s * z;
                let cx = bx + btn_w / 2.0;
                let cy = toggle_y + btn_h / 2.0;
                if dark {
                    // Moon: crescent (circle + smaller dark circle offset to the right)
                    push_rounded_quad(&mut ui_rnd, &mut ui_rnd_i,
                        cx - icon_r, cy - icon_r, icon_r * 2.0, icon_r * 2.0,
                        icon_r * 2.0, icon_r * 2.0, icon_r, icon_color);
                    // Dark cutout circle offset
                    push_rounded_quad(&mut ui_rnd, &mut ui_rnd_i,
                        cx - icon_r * 0.3, cy - icon_r * 1.0, icon_r * 1.6, icon_r * 1.6,
                        icon_r * 1.6, icon_r * 1.6, icon_r * 0.8, btn_bg);
                } else {
                    // Sun: circle + rays (small lines)
                    push_rounded_quad(&mut ui_rnd, &mut ui_rnd_i,
                        cx - icon_r * 0.7, cy - icon_r * 0.7, icon_r * 1.4, icon_r * 1.4,
                        icon_r * 1.4, icon_r * 1.4, icon_r * 0.7, icon_color);
                    // 4 rays
                    let ray_len = 3.0 * s * z;
                    let ray_w = 1.5 * s * z;
                    let offset = icon_r + 1.5 * s * z;
                    push_quad(&mut ui_bg, &mut ui_bg_i, cx - ray_w * 0.5, cy - offset - ray_len, ray_w, ray_len, icon_color); // top
                    push_quad(&mut ui_bg, &mut ui_bg_i, cx - ray_w * 0.5, cy + offset, ray_w, ray_len, icon_color); // bottom
                    push_quad(&mut ui_bg, &mut ui_bg_i, cx - offset - ray_len, cy - ray_w * 0.5, ray_len, ray_w, icon_color); // left
                    push_quad(&mut ui_bg, &mut ui_bg_i, cx + offset, cy - ray_w * 0.5, ray_len, ray_w, icon_color); // right
                }

                // Zoom percentage label (above the pill)
                let zoom_pct = format!("{}%", (self.canvas_zoom * 100.0) as u32);
                let label_w = zoom_pct.len() as f32 * self.atlas.cell_width;
                let label_x = bx + (btn_w - label_w) / 2.0;
                let label_y = by - self.atlas.cell_height - 6.0 * s * z;
                let label_color = ui_label.to_array();
                let mut lx = label_x;
                for c in zoom_pct.chars() {
                    if c != ' ' {
                        let glyph = self.atlas.get_or_rasterize(c, false, false, &self.gpu.queue);
                        if glyph.width > 0.0 && glyph.height > 0.0 {
                            let gx = lx + glyph.bearing_x;
                            let gy = label_y + (self.atlas.cell_height - glyph.bearing_y);
                            let base = ui_fg.len() as u32;
                            ui_fg.extend_from_slice(&[
                                TextVertex { position: [gx, gy], tex_coords: [glyph.tex_x, glyph.tex_y], color: label_color, bg_color: [0.0; 4] },
                                TextVertex { position: [gx + glyph.width, gy], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y], color: label_color, bg_color: [0.0; 4] },
                                TextVertex { position: [gx + glyph.width, gy + glyph.height], tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y + glyph.tex_h], color: label_color, bg_color: [0.0; 4] },
                                TextVertex { position: [gx, gy + glyph.height], tex_coords: [glyph.tex_x, glyph.tex_y + glyph.tex_h], color: label_color, bg_color: [0.0; 4] },
                            ]);
                            ui_fg_i.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
                        }
                    }
                    lx += self.atlas.cell_width;
                }

                // Draw rounded rects
                if !ui_rnd_i.is_empty() {
                    let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_rnd), usage: wgpu::BufferUsages::VERTEX });
                    let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_rnd_i), usage: wgpu::BufferUsages::INDEX });
                    pass.set_pipeline(&self.rounded_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.texture_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..ui_rnd_i.len() as u32, 0, 0..1);
                }
                // Draw bg quads (divider, icons)
                if !ui_bg_i.is_empty() {
                    let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_bg), usage: wgpu::BufferUsages::VERTEX });
                    let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_bg_i), usage: wgpu::BufferUsages::INDEX });
                    pass.set_pipeline(&self.bg_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.texture_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..ui_bg_i.len() as u32, 0, 0..1);
                }
                // Draw text (zoom label)
                if !ui_fg_i.is_empty() {
                    let vb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_fg), usage: wgpu::BufferUsages::VERTEX });
                    let ib = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&ui_fg_i), usage: wgpu::BufferUsages::INDEX });
                    pass.set_pipeline(&self.fg_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.texture_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..ui_fg_i.len() as u32, 0, 0..1);
                }
            }
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }
}

fn push_quad(verts: &mut Vec<TextVertex>, idxs: &mut Vec<u32>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    let base = verts.len() as u32;
    verts.extend_from_slice(&[
        TextVertex { position: [x, y], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: color },
        TextVertex { position: [x + w, y], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: color },
        TextVertex { position: [x + w, y + h], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: color },
        TextVertex { position: [x, y + h], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: color },
    ]);
    idxs.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
}

/// Push a rounded-rect quad. The shader uses:
/// - tex_coords = local position within the quad
/// - color.xy = full SDF rect size, color.z = corner radius
/// quad_w/quad_h = the actual quad size, sdf_w/sdf_h = the full rounded rect size for SDF
fn push_rounded_quad(
    verts: &mut Vec<TextVertex>, idxs: &mut Vec<u32>,
    x: f32, y: f32, quad_w: f32, quad_h: f32,
    sdf_w: f32, sdf_h: f32, radius: f32, bg_color: [f32; 4],
) {
    let base = verts.len() as u32;
    let c = [sdf_w, sdf_h, radius, 0.0];
    verts.extend_from_slice(&[
        TextVertex { position: [x, y], tex_coords: [0.0, 0.0], color: c, bg_color },
        TextVertex { position: [x + quad_w, y], tex_coords: [quad_w, 0.0], color: c, bg_color },
        TextVertex { position: [x + quad_w, y + quad_h], tex_coords: [quad_w, quad_h], color: c, bg_color },
        TextVertex { position: [x, y + quad_h], tex_coords: [0.0, quad_h], color: c, bg_color },
    ]);
    idxs.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
}
