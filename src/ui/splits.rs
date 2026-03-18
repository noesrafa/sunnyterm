/// Represents how a region is split.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitDirection {
    Vertical,   // side by side (Cmd+D)
    Horizontal, // stacked (Cmd+Shift+D)
}

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Tree node: either a leaf (pane) or a split containing two children.
pub enum LayoutNode {
    Leaf {
        pane_id: usize,
    },
    Split {
        direction: SplitDirection,
        ratio: f32, // 0.0 to 1.0, how much space the first child gets
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

/// Divider line for rendering.
pub struct Divider {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

const DIVIDER_SIZE: f32 = 2.0;

impl LayoutNode {
    pub fn single(pane_id: usize) -> Self {
        LayoutNode::Leaf { pane_id }
    }

    /// Compute rects for all leaf panes given the available area.
    pub fn compute_rects(&self, area: Rect, out: &mut Vec<(usize, Rect)>) {
        match self {
            LayoutNode::Leaf { pane_id } => {
                out.push((*pane_id, area));
            }
            LayoutNode::Split { direction, ratio, first, second } => {
                let (a, b) = split_rect(area, *direction, *ratio);
                first.compute_rects(a, out);
                second.compute_rects(b, out);
            }
        }
    }

    /// Collect divider lines for rendering.
    pub fn compute_dividers(&self, area: Rect, out: &mut Vec<Divider>) {
        if let LayoutNode::Split { direction, ratio, first, second } = self {
            let (a, b) = split_rect(area, *direction, *ratio);
            match direction {
                SplitDirection::Vertical => {
                    out.push(Divider {
                        x: a.x + a.w,
                        y: area.y,
                        w: DIVIDER_SIZE,
                        h: area.h,
                    });
                }
                SplitDirection::Horizontal => {
                    out.push(Divider {
                        x: area.x,
                        y: a.y + a.h,
                        w: area.w,
                        h: DIVIDER_SIZE,
                    });
                }
            }
            first.compute_dividers(a, out);
            second.compute_dividers(b, out);
        }
    }

    /// Split the leaf that contains the given pane_id.
    pub fn split_pane(&mut self, target_id: usize, new_id: usize, direction: SplitDirection) -> bool {
        match self {
            LayoutNode::Leaf { pane_id } if *pane_id == target_id => {
                let old = LayoutNode::Leaf { pane_id: target_id };
                let new = LayoutNode::Leaf { pane_id: new_id };
                *self = LayoutNode::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(old),
                    second: Box::new(new),
                };
                true
            }
            LayoutNode::Split { first, second, .. } => {
                first.split_pane(target_id, new_id, direction)
                    || second.split_pane(target_id, new_id, direction)
            }
            _ => false,
        }
    }

    /// Remove a pane and collapse the tree. Returns true if removed.
    pub fn remove_pane(&mut self, target_id: usize) -> bool {
        match self {
            LayoutNode::Leaf { .. } => false,
            LayoutNode::Split { first, second, .. } => {
                if let LayoutNode::Leaf { pane_id } = first.as_ref() {
                    if *pane_id == target_id {
                        *self = std::mem::replace(second.as_mut(), LayoutNode::Leaf { pane_id: 0 });
                        return true;
                    }
                }
                if let LayoutNode::Leaf { pane_id } = second.as_ref() {
                    if *pane_id == target_id {
                        *self = std::mem::replace(first.as_mut(), LayoutNode::Leaf { pane_id: 0 });
                        return true;
                    }
                }
                first.remove_pane(target_id) || second.remove_pane(target_id)
            }
        }
    }

    /// Get all pane IDs in order.
    pub fn pane_ids(&self) -> Vec<usize> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    fn collect_ids(&self, out: &mut Vec<usize>) {
        match self {
            LayoutNode::Leaf { pane_id } => out.push(*pane_id),
            LayoutNode::Split { first, second, .. } => {
                first.collect_ids(out);
                second.collect_ids(out);
            }
        }
    }
}

fn split_rect(area: Rect, direction: SplitDirection, ratio: f32) -> (Rect, Rect) {
    let half_div = DIVIDER_SIZE / 2.0;
    match direction {
        SplitDirection::Vertical => {
            let first_w = (area.w * ratio - half_div).max(0.0);
            let second_x = area.x + first_w + DIVIDER_SIZE;
            let second_w = (area.w - first_w - DIVIDER_SIZE).max(0.0);
            (
                Rect { x: area.x, y: area.y, w: first_w, h: area.h },
                Rect { x: second_x, y: area.y, w: second_w, h: area.h },
            )
        }
        SplitDirection::Horizontal => {
            let first_h = (area.h * ratio - half_div).max(0.0);
            let second_y = area.y + first_h + DIVIDER_SIZE;
            let second_h = (area.h - first_h - DIVIDER_SIZE).max(0.0);
            (
                Rect { x: area.x, y: area.y, w: area.w, h: first_h },
                Rect { x: area.x, y: second_y, w: area.w, h: second_h },
            )
        }
    }
}
