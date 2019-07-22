//! The bezier pen tool.

use druid::kurbo::Point;
use druid::MouseEvent;

use super::{Drag, MouseDelegate, Tool};
use crate::{Contents, MIN_POINT_DISTANCE};

/// The state of the pen.
#[derive(Debug, Clone, PartialEq)]
pub struct Pen;

impl MouseDelegate<Contents> for Pen {
    fn cancel(&mut self, canvas: &mut Contents) {
        canvas.selection_mut().clear();
    }

    fn left_down(&mut self, canvas: &mut Contents, event: &MouseEvent) -> bool {
        let vport = canvas.vport;
        if event.count == 1 {
            let point = match canvas.active_path() {
                Some(path)
                    if path.start_point().screen_dist(vport, event.pos) < MIN_POINT_DISTANCE =>
                {
                    path.start_point().to_screen(vport)
                }
                // lock to nearest vertical or horizontal axis if shift is pressed
                Some(path) if event.mods.shift => {
                    let last_point = path.points().last().unwrap().to_screen(vport);
                    axis_locked_point(event.pos, last_point)
                }
                _ => event.pos,
            };

            canvas.add_point(point);
        } else if event.count == 2 {
            canvas.selection_mut().clear();
        }
        true
    }

    fn left_up(&mut self, canvas: &mut Contents, _event: &MouseEvent) -> bool {
        if let Some(path) = canvas.active_path_mut() {
            if path.is_closed() || path.points().len() > 1 && !path.last_segment_is_curve() {
                path.clear_trailing();
            }
        }

        if canvas
            .active_path_mut()
            .map(|p| p.is_closed())
            .unwrap_or(false)
        {
            canvas.selection_mut().clear();
        }
        true
    }

    fn left_drag_changed(&mut self, canvas: &mut Contents, drag: Drag) -> bool {
        let Drag { start, current, .. } = drag;
        let handle_point = if current.mods.shift {
            axis_locked_point(current.pos, start.pos)
        } else {
            current.pos
        };
        canvas.update_for_drag(handle_point);
        true
    }
}

impl Tool for Pen {
    fn boxed_clone(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }

    fn name(&self) -> &str {
        "pen"
    }
}

impl Pen {
    pub(crate) fn new() -> Self {
        Pen
    }
}

/// Lock the smallest axis of `point` (from `prev`) to that axis on `prev`.
/// (aka shift + click)
fn axis_locked_point(point: Point, prev: Point) -> Point {
    let dxy = prev - point;
    if dxy.x.abs() > dxy.y.abs() {
        Point::new(point.x, prev.y)
    } else {
        Point::new(prev.x, point.y)
    }
}
