/// A floating terminal tile on the canvas.
pub struct Tile {
    pub id: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub name: String,
}

pub const TITLE_BAR_HEIGHT: f32 = 28.0;
const MIN_TILE_W: f32 = 200.0;
const MIN_TILE_H: f32 = 120.0;
const RESIZE_HANDLE: f32 = 18.0;
const SNAP_GRID: f32 = 24.0; // matches dot grid spacing

fn snap(val: f32, grid: f32) -> f32 {
    (val / grid).round() * grid
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragMode {
    Move,
    Resize,
}

pub struct DragState {
    pub tile_id: usize,
    pub mode: DragMode,
    pub start_mouse: (f32, f32),
    pub start_tile: (f32, f32, f32, f32),
}

pub struct Canvas {
    pub tiles: Vec<Tile>,
    pub focus_order: Vec<usize>,
    next_id: usize,
    pub drag: Option<DragState>,
}

impl Canvas {
    pub fn new() -> Self {
        Self {
            tiles: Vec::new(),
            focus_order: Vec::new(),
            next_id: 0,
            drag: None,
        }
    }

    pub fn spawn(&mut self, x: f32, y: f32, w: f32, h: f32) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let name = format!("Terminal {}", id + 1);
        self.tiles.push(Tile { id, x, y, w, h, name });
        self.focus_order.push(id);
        id
    }

    pub fn spawn_named(&mut self, x: f32, y: f32, w: f32, h: f32, name: String) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.tiles.push(Tile { id, x, y, w, h, name });
        self.focus_order.push(id);
        id
    }

    pub fn remove(&mut self, id: usize) {
        self.tiles.retain(|t| t.id != id);
        self.focus_order.retain(|&i| i != id);
    }

    pub fn focused_id(&self) -> Option<usize> {
        self.focus_order.last().copied()
    }

    pub fn focus(&mut self, id: usize) {
        self.focus_order.retain(|&i| i != id);
        self.focus_order.push(id);
    }

    pub fn tile(&self, id: usize) -> Option<&Tile> {
        self.tiles.iter().find(|t| t.id == id)
    }

    pub fn tile_mut(&mut self, id: usize) -> Option<&mut Tile> {
        self.tiles.iter_mut().find(|t| t.id == id)
    }

    pub fn draw_order(&self) -> Vec<usize> {
        self.focus_order.clone()
    }

    /// Hit test the topmost tile at (x,y).
    /// The tile's total rect is: x, y .. x+w, y+bar_h+h
    /// (title bar at top, content below)
    /// Hit test the topmost tile at (x,y).
    /// Returns (tile_id, in_title, in_resize, in_close).
    pub fn hit_test(&self, x: f32, y: f32, scale: f32) -> Option<(usize, bool, bool, bool)> {
        let bar_h = TITLE_BAR_HEIGHT * scale;
        let handle = RESIZE_HANDLE * scale;
        let close_size = 28.0 * scale;
        let close_margin = 2.0 * scale;

        for &id in self.focus_order.iter().rev() {
            if let Some(tile) = self.tile(id) {
                let total_h = tile.h + bar_h;
                if x >= tile.x && x < tile.x + tile.w && y >= tile.y && y < tile.y + total_h {
                    let in_title = y < tile.y + bar_h;
                    let in_resize = x >= tile.x + tile.w - handle && y >= tile.y + total_h - handle;
                    // Close button: top-right corner of title bar
                    let close_x = tile.x + tile.w - close_size - close_margin;
                    let close_y = tile.y + (bar_h - close_size) / 2.0;
                    let in_close = in_title
                        && x >= close_x && x < close_x + close_size
                        && y >= close_y && y < close_y + close_size;
                    return Some((id, in_title, in_resize, in_close));
                }
            }
        }
        None
    }

    pub fn start_drag(&mut self, id: usize, mode: DragMode, mouse_x: f32, mouse_y: f32) {
        if let Some(tile) = self.tile(id) {
            self.drag = Some(DragState {
                tile_id: id,
                mode,
                start_mouse: (mouse_x, mouse_y),
                start_tile: (tile.x, tile.y, tile.w, tile.h),
            });
        }
    }

    pub fn update_drag(&mut self, mouse_x: f32, mouse_y: f32, scale: f32) -> bool {
        let Some(drag) = &self.drag else { return false };
        let dx = mouse_x - drag.start_mouse.0;
        let dy = mouse_y - drag.start_mouse.1;
        let (sx, sy, sw, sh) = drag.start_tile;
        let id = drag.tile_id;
        let mode = drag.mode;
        let grid = SNAP_GRID * scale;

        if let Some(tile) = self.tile_mut(id) {
            match mode {
                DragMode::Move => {
                    tile.x = snap(sx + dx, grid);
                    tile.y = snap(sy + dy, grid);
                }
                DragMode::Resize => {
                    let min_w = MIN_TILE_W * scale;
                    let min_h = MIN_TILE_H * scale;
                    tile.w = snap((sw + dx).max(min_w), grid);
                    tile.h = snap((sh + dy).max(min_h), grid);
                }
            }
            return true;
        }
        false
    }

    pub fn end_drag(&mut self) {
        self.drag = None;
    }

    pub fn rename_focused(&mut self, name: String) {
        if let Some(id) = self.focused_id() {
            if let Some(tile) = self.tile_mut(id) {
                tile.name = name;
            }
        }
    }
}
