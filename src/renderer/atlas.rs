use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight};
use std::collections::HashMap;

const ATLAS_SIZE: u32 = 4096;

#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    pub tex_x: f32,
    pub tex_y: f32,
    pub tex_w: f32,
    pub tex_h: f32,
    pub width: f32,
    pub height: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
}

pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    cache: HashMap<(char, bool, bool), GlyphInfo>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    font_system: FontSystem,
    swash_cache: SwashCache,
    pub cell_width: f32,
    pub cell_height: f32,
    physical_font_size: f32,
    base_font_size: f32,
    scale_factor: f32,
    render_zoom: f32,
}

impl GlyphAtlas {
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        font_size: f32,
        scale_factor: f32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let mut font_system = FontSystem::new();

        // Load embedded JetBrains Mono font directly
        let font_medium = include_bytes!("../../assets/fonts/JetBrainsMono-Medium.ttf");
        let font_bold = include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf");
        font_system.db_mut().load_font_data(font_medium.to_vec());
        font_system.db_mut().load_font_data(font_bold.to_vec());

        let swash_cache = SwashCache::new();

        // Render at physical pixel size for Retina
        let physical_font_size = font_size * scale_factor;
        let line_height = (physical_font_size * 1.3).ceil();

        // Measure cell width from the glyph advance
        let metrics = Metrics::new(physical_font_size, line_height);
        let mut buffer = Buffer::new(&mut font_system, metrics);
        let attrs = Attrs::new()
            .family(Family::Name("JetBrains Mono"))
            .weight(Weight::BOLD);
        buffer.set_text(&mut font_system, "M", attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut font_system, false);

        let mut cell_width = (physical_font_size * 0.6).ceil();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                cell_width = glyph.w.ceil();
                break;
            }
            break;
        }

        eprintln!(
            "[sunnyterm] font: physical_size={physical_font_size}, cell_w={cell_width}, cell_h={line_height}, scale={scale_factor}"
        );

        Self {
            texture,
            view,
            sampler,
            cache: HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            font_system,
            swash_cache,
            cell_width,
            cell_height: line_height,
            physical_font_size,
            base_font_size: font_size,
            scale_factor,
            render_zoom: 1.0,
        }
    }

    pub fn get_or_rasterize(
        &mut self,
        c: char,
        bold: bool,
        italic: bool,
        queue: &wgpu::Queue,
    ) -> GlyphInfo {
        let key = (c, bold, italic);
        if let Some(info) = self.cache.get(&key) {
            return *info;
        }

        let metrics = Metrics::new(self.physical_font_size, self.cell_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);

        let weight = if bold { Weight::EXTRA_BOLD } else { Weight::BOLD };
        let style = if italic { Style::Italic } else { Style::Normal };
        let attrs = Attrs::new()
            .family(Family::Name("JetBrains Mono"))
            .weight(weight)
            .style(style);

        let s = c.to_string();
        buffer.set_text(&mut self.font_system, &s, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut glyph_info = GlyphInfo {
            tex_x: 0.0, tex_y: 0.0,
            tex_w: 0.0, tex_h: 0.0,
            width: 0.0, height: 0.0,
            bearing_x: 0.0, bearing_y: 0.0,
        };

        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                if let Some(image) = self.swash_cache.get_image(&mut self.font_system, physical.cache_key) {
                    let w = image.placement.width;
                    let h = image.placement.height;

                    if w == 0 || h == 0 {
                        break;
                    }

                    if self.cursor_x + w > ATLAS_SIZE {
                        self.cursor_x = 0;
                        self.cursor_y += self.row_height + 1;
                        self.row_height = 0;
                    }

                    if self.cursor_y + h > ATLAS_SIZE {
                        log::warn!("Glyph atlas full!");
                        break;
                    }

                    let pixels: Vec<u8> = match image.content {
                        cosmic_text::SwashContent::Mask => image.data.clone(),
                        cosmic_text::SwashContent::Color => {
                            image.data.chunks(4).map(|c| c.get(3).copied().unwrap_or(255)).collect()
                        }
                        cosmic_text::SwashContent::SubpixelMask => {
                            image.data.chunks(3).map(|c| {
                                let sum: u16 = c.iter().map(|&v| v as u16).sum();
                                (sum / 3) as u8
                            }).collect()
                        }
                    };

                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &self.texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: self.cursor_x,
                                y: self.cursor_y,
                                z: 0,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        &pixels,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(w),
                            rows_per_image: Some(h),
                        },
                        wgpu::Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                    );

                    let z = self.render_zoom;
                    glyph_info = GlyphInfo {
                        tex_x: self.cursor_x as f32 / ATLAS_SIZE as f32,
                        tex_y: self.cursor_y as f32 / ATLAS_SIZE as f32,
                        tex_w: w as f32 / ATLAS_SIZE as f32,
                        tex_h: h as f32 / ATLAS_SIZE as f32,
                        width: w as f32 / z,
                        height: h as f32 / z,
                        bearing_x: image.placement.left as f32 / z,
                        bearing_y: image.placement.top as f32 / z,
                    };

                    self.cursor_x += w + 1;
                    self.row_height = self.row_height.max(h);
                }
                break;
            }
        }

        self.cache.insert(key, glyph_info);
        glyph_info
    }

    /// Update the render zoom. Re-rasterizes all cached glyphs if zoom changed enough.
    /// cell_width/cell_height stay the same (they're in canvas coords).
    /// Only the bitmap resolution changes.
    pub fn set_zoom(&mut self, zoom: f32, queue: &wgpu::Queue) {
        // Quantize zoom to avoid re-rasterizing on tiny changes
        let quantized = (zoom * 4.0).round() / 4.0;
        if (quantized - self.render_zoom).abs() < 0.01 {
            return;
        }
        self.render_zoom = quantized;
        self.physical_font_size = self.base_font_size * self.scale_factor * quantized;

        // Clear atlas and re-rasterize will happen lazily on next get_or_rasterize
        self.cache.clear();
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_height = 0;

        // Clear the texture
        let zeros = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize];
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &zeros,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_SIZE),
                rows_per_image: Some(ATLAS_SIZE),
            },
            wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }
}
