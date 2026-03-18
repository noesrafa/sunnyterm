struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) bg_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) bg_color: vec4<f32>,
};

struct Uniforms {
    projection: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var glyph_texture: texture_2d<f32>;
@group(1) @binding(1)
var glyph_sampler: sampler;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = uniforms.projection * vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    out.bg_color = in.bg_color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(glyph_texture, glyph_sampler, in.tex_coords).r;
    let fg = vec4<f32>(in.color.rgb, in.color.a * alpha);
    return mix(in.bg_color, fg, fg.a);
}

// Background-only pass (no texture sampling)
@fragment
fn fs_bg_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.bg_color;
}

// Rounded rect background pass
// tex_coords.xy = local position within the rect (0..size)
// color.xy = rect size, color.z = corner radius
@fragment
fn fs_rounded_bg(in: VertexOutput) -> @location(0) vec4<f32> {
    let pos = in.tex_coords;
    let size = in.color.xy;
    let radius = in.color.z;

    // SDF for rounded rectangle
    let half = size * 0.5;
    let p = abs(pos - half) - half + vec2<f32>(radius);
    let d = length(max(p, vec2<f32>(0.0))) - radius;

    // Sharp edge with minimal AA to avoid ghosting during pan
    let alpha = 1.0 - smoothstep(-0.5, 0.5, d);

    // Snap alpha to 0 or 1 for very low/high values to reduce blending artifacts
    let snapped = select(alpha, 0.0, alpha < 0.02);
    let final_alpha = select(snapped, 1.0, snapped > 0.98);

    return vec4<f32>(in.bg_color.rgb, in.bg_color.a * final_alpha);
}
