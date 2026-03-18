use crate::renderer::text::TextVertex;
use crate::ui::theme::Theme;
use std::time::Instant;

pub struct CursorRenderer {
    pub visible: bool,
    pub blink: bool,
    last_toggle: Instant,
    blink_on: bool,
}

impl CursorRenderer {
    pub fn new(blink: bool) -> Self {
        Self {
            visible: true,
            blink,
            last_toggle: Instant::now(),
            blink_on: true,
        }
    }

    pub fn update(&mut self) {
        if self.blink {
            let elapsed = self.last_toggle.elapsed().as_millis();
            if elapsed > 530 {
                self.blink_on = !self.blink_on;
                self.last_toggle = Instant::now();
            }
        }
    }

    pub fn reset_blink(&mut self) {
        self.blink_on = true;
        self.last_toggle = Instant::now();
    }

    pub fn build_vertices(
        &self,
        cursor_row: usize,
        cursor_col: usize,
        cell_width: f32,
        cell_height: f32,
        padding: f32,
        cursor_style: &str,
        theme: &Theme,
    ) -> (Vec<TextVertex>, Vec<u32>) {
        if !self.visible || (self.blink && !self.blink_on) {
            return (vec![], vec![]);
        }

        let x = padding + cursor_col as f32 * cell_width;
        let y = padding + cursor_row as f32 * cell_height;
        let color = theme.cursor.to_array();

        let beam_width = (cell_width * 0.12).max(2.0);
        let (w, h) = match cursor_style {
            "beam" => (beam_width, cell_height),
            "underline" => (cell_width, beam_width),
            _ => (cell_width, cell_height), // block
        };

        let oy = match cursor_style {
            "underline" => cell_height - beam_width,
            _ => 0.0,
        };

        let vertices = vec![
            TextVertex { position: [x, y + oy], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: color },
            TextVertex { position: [x + w, y + oy], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: color },
            TextVertex { position: [x + w, y + oy + h], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: color },
            TextVertex { position: [x, y + oy + h], tex_coords: [0.0; 2], color: [0.0; 4], bg_color: color },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];

        (vertices, indices)
    }
}
