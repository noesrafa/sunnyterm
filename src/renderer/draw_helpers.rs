use crate::renderer::text::TextVertex;

pub struct DrawBatch {
    pub rounded_verts: Vec<TextVertex>,
    pub rounded_indices: Vec<u32>,
    pub bg_verts: Vec<TextVertex>,
    pub bg_indices: Vec<u32>,
    pub fg_verts: Vec<TextVertex>,
    pub fg_indices: Vec<u32>,
}

impl DrawBatch {
    pub fn new() -> Self {
        Self {
            rounded_verts: Vec::new(),
            rounded_indices: Vec::new(),
            bg_verts: Vec::new(),
            bg_indices: Vec::new(),
            fg_verts: Vec::new(),
            fg_indices: Vec::new(),
        }
    }
}

pub fn push_quad(verts: &mut Vec<TextVertex>, idxs: &mut Vec<u32>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
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
pub fn push_rounded_quad(
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
