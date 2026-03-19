use crate::app::Selection;
use crate::renderer::atlas::GlyphAtlas;
use crate::terminal::grid::Grid;
use crate::ui::theme::Theme;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TextVertex {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
    pub color: [f32; 4],
    pub bg_color: [f32; 4],
}

impl TextVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TextVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct TextRenderer {
    pub bg_vertices: Vec<TextVertex>,
    pub bg_indices: Vec<u32>,
    pub fg_vertices: Vec<TextVertex>,
    pub fg_indices: Vec<u32>,
    pub sel_vertices: Vec<TextVertex>,
    pub sel_indices: Vec<u32>,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            bg_vertices: Vec::new(),
            bg_indices: Vec::new(),
            fg_vertices: Vec::new(),
            fg_indices: Vec::new(),
            sel_vertices: Vec::new(),
            sel_indices: Vec::new(),
        }
    }

    /// Build selection highlight quads for the given selection.
    pub fn build_selection(
        &mut self,
        selection: Option<&Selection>,
        grid: &Grid,
        atlas: &GlyphAtlas,
        padding: f32,
        sel_color: [f32; 4],
    ) {
        self.sel_vertices.clear();
        self.sel_indices.clear();
        let sel = match selection {
            Some(s) => s,
            None => return,
        };
        let cell_w = atlas.cell_width;
        let cell_h = atlas.cell_height;
        let ((sr, sc), (er, ec)) = sel.ordered();

        for row in sr..=er {
            if row >= grid.rows { break; }
            let col_start = if row == sr { sc } else { 0 };
            let col_end = if row == er { ec + 1 } else { grid.cols };
            let col_end = col_end.min(grid.cols);
            if col_start >= col_end { continue; }

            let x = padding + col_start as f32 * cell_w;
            let y = padding + row as f32 * cell_h;
            let w = (col_end - col_start) as f32 * cell_w;

            let idx = self.sel_vertices.len() as u32;
            self.sel_vertices.extend_from_slice(&[
                TextVertex { position: [x, y], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: sel_color },
                TextVertex { position: [x + w, y], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: sel_color },
                TextVertex { position: [x + w, y + cell_h], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: sel_color },
                TextVertex { position: [x, y + cell_h], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: sel_color },
            ]);
            self.sel_indices.extend_from_slice(&[idx, idx + 1, idx + 2, idx, idx + 2, idx + 3]);
        }
    }

    pub fn build_vertices(
        &mut self,
        grid: &Grid,
        atlas: &mut GlyphAtlas,
        theme: &Theme,
        padding: f32,
        queue: &wgpu::Queue,
    ) {
        self.build_vertices_range(grid, atlas, theme, padding, queue, 0..grid.rows, 0.0);
    }

    /// Render a range of grid rows, with a y_offset applied to all vertices.
    pub fn build_vertices_range(
        &mut self,
        grid: &Grid,
        atlas: &mut GlyphAtlas,
        theme: &Theme,
        padding: f32,
        queue: &wgpu::Queue,
        row_range: std::ops::Range<usize>,
        y_offset: f32,
    ) {
        self.bg_vertices.clear();
        self.bg_indices.clear();
        self.fg_vertices.clear();
        self.fg_indices.clear();

        let cell_w = atlas.cell_width;
        let cell_h = atlas.cell_height;

        let default_cell = crate::terminal::cell::Cell::default();
        for (local_row, row) in row_range.enumerate() {
            if row >= grid.rows { break; }
            let line = grid.display_line(row);
            for col in 0..grid.cols {
                let cell = if col < line.len() { &line[col] } else { &default_cell };
                let x = padding + col as f32 * cell_w;
                let y = y_offset + padding + local_row as f32 * cell_h;

                let (fg_color, bg_color) = if cell.attrs.inverse {
                    (
                        cell.attrs.bg.to_color(theme, false),
                        cell.attrs.fg.to_color(theme, true),
                    )
                } else {
                    (
                        cell.attrs.fg.to_color(theme, true),
                        cell.attrs.bg.to_color(theme, false),
                    )
                };

                // Background quad
                let bg_idx = self.bg_vertices.len() as u32;
                let bg_c = bg_color.to_array();
                self.bg_vertices.extend_from_slice(&[
                    TextVertex { position: [x, y], tex_coords: [0.0, 0.0], color: [0.0; 4], bg_color: bg_c },
                    TextVertex { position: [x + cell_w, y], tex_coords: [0.0, 0.0], color: [0.0; 4], bg_color: bg_c },
                    TextVertex { position: [x + cell_w, y + cell_h], tex_coords: [0.0, 0.0], color: [0.0; 4], bg_color: bg_c },
                    TextVertex { position: [x, y + cell_h], tex_coords: [0.0, 0.0], color: [0.0; 4], bg_color: bg_c },
                ]);
                self.bg_indices.extend_from_slice(&[bg_idx, bg_idx + 1, bg_idx + 2, bg_idx, bg_idx + 2, bg_idx + 3]);

                // Foreground glyph
                if cell.c != ' ' && cell.c != '\0' {
                    let glyph = atlas.get_or_rasterize(cell.c, cell.attrs.bold, cell.attrs.italic, queue);
                    if glyph.width > 0.0 && glyph.height > 0.0 {
                        let gx = (x + glyph.bearing_x).round();
                        let gy = (y + (cell_h - glyph.bearing_y)).round();

                        let fg_c = fg_color.to_array();
                        let fg_idx = self.fg_vertices.len() as u32;
                        self.fg_vertices.extend_from_slice(&[
                            TextVertex {
                                position: [gx, gy],
                                tex_coords: [glyph.tex_x, glyph.tex_y],
                                color: fg_c,
                                bg_color: [0.0; 4],
                            },
                            TextVertex {
                                position: [gx + glyph.width, gy],
                                tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y],
                                color: fg_c,
                                bg_color: [0.0; 4],
                            },
                            TextVertex {
                                position: [gx + glyph.width, gy + glyph.height],
                                tex_coords: [glyph.tex_x + glyph.tex_w, glyph.tex_y + glyph.tex_h],
                                color: fg_c,
                                bg_color: [0.0; 4],
                            },
                            TextVertex {
                                position: [gx, gy + glyph.height],
                                tex_coords: [glyph.tex_x, glyph.tex_y + glyph.tex_h],
                                color: fg_c,
                                bg_color: [0.0; 4],
                            },
                        ]);
                        self.fg_indices.extend_from_slice(&[fg_idx, fg_idx + 1, fg_idx + 2, fg_idx, fg_idx + 2, fg_idx + 3]);
                    }
                }
            }
        }
    }
}
