use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use super::ViewPort;
use druid::kurbo::{BezPath, Point, Vec2};
use druid::Data;

/// We give paths & points unique integer identifiers.
fn next_id() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Hash, Eq, Ord)]
pub struct PointId {
    pub(crate) path: usize,
    pub(crate) point: usize,
}

impl std::cmp::PartialEq<Path> for PointId {
    fn eq(&self, other: &Path) -> bool {
        self.path == other.id
    }
}

impl std::cmp::PartialEq<PointId> for Path {
    fn eq(&self, other: &PointId) -> bool {
        self.id == other.path
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointType {
    OnCurve,
    OnCurveSmooth,
    OffCurve,
}

/// A point in design space.
///
/// This type should only be constructed through a function that has access to,
/// and takes account of, the current viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DPoint {
    x: f64,
    y: f64,
}

impl DPoint {
    fn new(x: f64, y: f64) -> DPoint {
        assert!(x.is_finite() && y.is_finite() && x.fract() == 0. && y.fract() == 0.);
        DPoint { x, y }
    }

    pub fn from_screen(point: Point, vport: ViewPort) -> DPoint {
        let point = vport.transform().inverse() * point;
        DPoint::new(point.x.round(), point.y.round())
    }

    pub fn to_screen(self, vport: ViewPort) -> Point {
        vport.transform()
            * Point {
                x: self.x,
                y: self.y,
            }
    }

    /// Create a new `DPoint` from a `Point` in design space. This should only
    /// be used to convert back to a `DPoint` after using `Point` to do vector
    /// math in design space.
    pub fn from_raw(point: impl Into<Point>) -> DPoint {
        let point = point.into();
        DPoint::new(point.x.round(), point.y.round())
    }

    fn to_raw(self) -> Point {
        Point::new(self.x, self.y)
    }
}

/// A vector in design space, used for nudging & dragging
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DVec2 {
    x: f64,
    y: f64,
}

impl DVec2 {
    pub fn from_screen(vec2: Vec2, vport: ViewPort) -> DVec2 {
        let vec2 = vport.transform().inverse() * vec2.to_point();
        DVec2 {
            x: vec2.x.round(),
            y: vec2.y.round(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PathPoint {
    pub id: PointId,
    point: DPoint,
    pub typ: PointType,
}

#[derive(Debug, Clone)]
pub struct Path {
    id: usize,
    points: std::sync::Arc<Vec<PathPoint>>,
    trailing: Option<DPoint>,
    closed: bool,
    bezier: RefCell<BezPath>,
    bezier_stale: Cell<bool>,
}

impl PointType {
    pub fn is_on_curve(self) -> bool {
        match self {
            PointType::OnCurve | PointType::OnCurveSmooth => true,
            PointType::OffCurve => false,
        }
    }
}

impl PathPoint {
    fn off_curve(path: usize, point: DPoint) -> PathPoint {
        let id = PointId {
            path,
            point: next_id(),
        };
        PathPoint {
            id,
            point,
            typ: PointType::OffCurve,
        }
    }

    fn on_curve(path: usize, point: DPoint) -> PathPoint {
        let id = PointId {
            path,
            point: next_id(),
        };
        PathPoint {
            id,
            point,
            typ: PointType::OnCurve,
        }
    }

    pub fn is_on_curve(&self) -> bool {
        self.typ.is_on_curve()
    }

    /// The distance, in screen space, from this `PathPoint` to `point`, a point
    /// in screen space.
    pub fn screen_dist(&self, view: ViewPort, point: Point) -> f64 {
        let this = view.transform() * self.raw_point();
        this.distance(point)
    }

    /// Convert this point to point in screen space.
    pub fn to_screen(&self, view: ViewPort) -> Point {
        view.transform() * self.raw_point()
    }

    /// Return the raw design space point, as a `Point`. This is only used
    /// privately, so that we use methods on `Point`.
    fn raw_point(&self) -> Point {
        self.point.to_raw()
    }
}

impl Path {
    pub fn new(point: DPoint) -> Path {
        let id = next_id();
        let start = PathPoint::on_curve(id, point);

        Path {
            id,
            points: std::sync::Arc::new(vec![start]),
            closed: false,
            trailing: None,
            bezier: RefCell::new(BezPath::new()),
            bezier_stale: Cell::new(false),
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn points(&self) -> &[PathPoint] {
        self.points.as_slice()
    }

    fn points_mut(&mut self) -> &mut Vec<PathPoint> {
        self.bezier_stale.set(true);
        std::sync::Arc::make_mut(&mut self.points)
    }

    pub fn trailing(&self) -> Option<&DPoint> {
        self.trailing.as_ref()
    }

    pub fn clear_trailing(&mut self) {
        self.trailing = None;
    }

    /// Whether we should draw the 'trailing' control point & handle.
    /// We always do this for the first point, if it exists; otherwise
    /// we do it for curve points only.
    pub fn should_draw_trailing(&self) -> bool {
        self.points.len() == 1 || self.last_segment_is_curve()
    }

    /// Returns the start point of the path.
    pub fn start_point(&self) -> &PathPoint {
        if self.closed {
            self.points.last().unwrap()
        } else {
            self.points.first().unwrap()
        }
    }

    pub fn bezier(&self) -> std::cell::Ref<BezPath> {
        if self.bezier_stale.replace(false) {
            *self.bezier.borrow_mut() = self.make_bezier();
        }
        self.bezier.borrow()
    }

    fn make_bezier(&self) -> BezPath {
        let mut bez = BezPath::new();
        bez.move_to(self.start_point().raw_point());
        let mut i = if self.closed { 0 } else { 1 };
        self.debug_print_points();

        while i < self.points.len() {
            if self.points[i].is_on_curve() {
                bez.line_to(self.points[i].raw_point());
                i += 1;
            } else {
                bez.curve_to(
                    self.points[i].raw_point(),
                    self.points[i + 1].raw_point(),
                    self.points[self.next_idx(i + 1)].raw_point(),
                );
                i += 3;
            }
        }
        if self.closed {
            bez.close_path();
        }
        bez
    }

    pub fn screen_dist(&self, vport: ViewPort, point: Point) -> f64 {
        use std::ops::Deref;
        let screen_bez = vport.transform() * self.bezier().deref();
        let (_, x, y) = screen_bez.nearest(point, 0.1);
        dbg!(Vec2::new(x, y).hypot())
    }

    /// Appends a point. Called when the user clicks. This point is always a corner;
    /// if the user drags it will be converted to a curve then.
    ///
    /// Returns the id of the newly added point, or the start/end point if this
    /// closes the path.
    pub fn append_point(&mut self, point: DPoint) -> PointId {
        if !self.closed && point == self.points[0].point {
            return self.close();
        }
        let new = PathPoint::on_curve(self.id, point);
        self.points_mut().push(new);
        new.id
    }

    pub fn nudge_points(&mut self, points: &[PointId], v: DVec2) {
        let mut to_nudge = HashSet::new();
        for point in points {
            let idx = match self.points.iter().position(|p| p.id == *point) {
                Some(idx) => idx,
                None => continue,
            };
            to_nudge.insert(idx);
            if self.points[idx].is_on_curve() {
                let prev = self.prev_idx(idx);
                let next = self.next_idx(idx);
                if !self.points[prev].is_on_curve() {
                    to_nudge.insert(prev);
                }
                if !self.points[next].is_on_curve() {
                    to_nudge.insert(next);
                }
            }
        }

        for idx in &to_nudge {
            self.nudge_point(*idx, v);
            if !self.points[*idx].is_on_curve() {
                if let Some((on_curve, handle)) = self.tangent_handle(*idx) {
                    if !to_nudge.contains(&handle) {
                        self.adjust_handle_angle(*idx, on_curve, handle);
                    }
                }
            }
        }
    }

    fn nudge_point(&mut self, idx: usize, v: DVec2) {
        self.points_mut()[idx].point.x += v.x;
        self.points_mut()[idx].point.y += v.y;
    }

    /// Returns the index for the on_curve point and the 'other' handle
    /// for an offcurve point, if it exists.
    fn tangent_handle(&self, idx: usize) -> Option<(usize, usize)> {
        assert!(!self.points[idx].is_on_curve());
        let prev = self.prev_idx(idx);
        let next = self.next_idx(idx);
        if self.points[prev].typ == PointType::OnCurveSmooth {
            let prev2 = self.prev_idx(prev);
            if !self.points[prev2].is_on_curve() {
                return Some((prev, prev2));
            }
        } else if self.points[next].typ == PointType::OnCurveSmooth {
            let next2 = self.next_idx(next);
            if !self.points[next2].is_on_curve() {
                return Some((next, next2));
            }
        }
        None
    }

    /// Update a tangent handle in response to the movement of the partner handle.
    /// `bcp1` is the handle that has moved, and `bcp2` is the handle that needs
    /// to be adjusted.
    fn adjust_handle_angle(&mut self, bcp1: usize, on_curve: usize, bcp2: usize) {
        let new_angle = (self.points[bcp1].raw_point() - self.points[on_curve].raw_point()) * -1.0;
        let new_angle = new_angle.normalize();
        let handle_len = (self.points[bcp2].raw_point() - self.points[on_curve].raw_point())
            .hypot()
            .abs();
        let new_pos = self.points[on_curve].raw_point() + new_angle * handle_len;
        dbg!(new_angle, handle_len, new_pos);
        self.points_mut()[bcp2].point = DPoint::from_raw(new_pos);
    }

    fn debug_print_points(&self) {
        eprintln!(
            "path {}, len {} closed {}",
            self.id,
            self.points.len(),
            self.closed
        );
        for point in self.points.iter() {
            eprintln!(
                "[{}, {}]: {:?} {:?}",
                point.id.path, point.id.point, point.point, point.typ
            );
        }
    }

    pub fn delete_points(&mut self, points: &[PointId]) {
        eprintln!("deleting {:?}", points);
        for point in points {
            self.delete_point(*point)
        }
    }

    //FIXME: this is currently buggy :(
    fn delete_point(&mut self, point_id: PointId) {
        let idx = match self.points.iter().position(|p| p.id == point_id) {
            Some(idx) => idx,
            None => return,
        };

        let prev_idx = self.prev_idx(idx);
        let next_idx = self.next_idx(idx);

        eprintln!("deleting {:?}", point_id);
        self.debug_print_points();

        match self.points[idx].typ {
            PointType::OffCurve => {
                // delete both of the off curve points for this segment
                let other_id = if self.points[prev_idx].typ == PointType::OffCurve {
                    self.points[prev_idx].id
                } else {
                    assert!(self.points[next_idx].typ == PointType::OffCurve);
                    self.points[next_idx].id
                };
                self.points_mut()
                    .retain(|p| p.id != point_id && p.id != other_id);
            }
            _on_curve if self.points.len() == 1 => {
                self.points_mut().clear();
            }
            // with less than 4 points they must all be on curve
            _on_curve if self.points.len() == 4 => {
                self.points_mut()
                    .retain(|p| p.is_on_curve() && p.id != point_id);
            }

            _on_curve if self.points[prev_idx].is_on_curve() => {
                // this is a line segment
                self.points_mut().remove(idx);
            }
            _on_curve if self.points[next_idx].is_on_curve() => {
                // if we neighbour a corner point, leave handles (neighbour becomes curve)
                self.points_mut().remove(idx);
            }
            _ => {
                assert!(self.points.len() > 4);
                let prev = self.points[prev_idx];
                let next = self.points[next_idx];
                assert!(!prev.is_on_curve() && !next.is_on_curve());
                let to_del = [prev.id, next.id, point_id];
                self.points_mut().retain(|p| !to_del.contains(&p.id));
                if self.points.len() == 3 {
                    self.points_mut().retain(|p| p.is_on_curve());
                }
            }
        }

        // normalize our representation
        let len = self.points.len();
        if len > 2 && !self.points[0].is_on_curve() && !self.points[len - 1].is_on_curve() {
            self.points_mut().rotate_left(1);
        }

        // if we have fewer than three on_curve points we are open.
        if self.points.len() < 3 {
            self.closed = false;
        }
    }

    /// Called when the user drags (modifying the bezier control points) after clicking.
    pub fn update_for_drag(&mut self, handle: DPoint) {
        assert!(!self.points.is_empty());
        if !self.last_segment_is_curve() {
            self.convert_last_to_curve(handle);
        } else {
            self.update_trailing(handle);
        }
    }

    pub fn last_segment_is_curve(&self) -> bool {
        let len = self.points.len();
        len > 2 && !self.points[len - 2].is_on_curve()
    }

    pub fn toggle_on_curve_point_type(&mut self, id: PointId) {
        let idx = self.idx_for_point(id).unwrap();
        let has_ctrl = !self.points[self.prev_idx(idx)].is_on_curve()
            || !self.points[self.next_idx(idx)].is_on_curve();
        let point = &mut self.points_mut()[idx];
        point.typ = match point.typ {
            PointType::OnCurve if has_ctrl => PointType::OnCurveSmooth,
            PointType::OnCurveSmooth => PointType::OnCurve,
            other => other,
        }
    }

    /// If the user drags after mousedown, we convert the last point to a curve.
    fn convert_last_to_curve(&mut self, handle: DPoint) {
        assert!(!self.points.is_empty());
        if self.points.len() > 1 {
            let mut prev = self.points_mut().pop().unwrap();
            prev.typ = PointType::OnCurveSmooth;
            let p1 = self
                .trailing
                .take()
                .unwrap_or(self.points.last().unwrap().point);
            let p2 = prev.raw_point() - (handle.to_raw() - prev.raw_point());
            let p2 = DPoint::from_raw(p2);
            let pts = &[
                PathPoint::off_curve(self.id, p1),
                PathPoint::off_curve(self.id, p2),
                prev,
            ];
            self.points_mut().extend(pts);
        }
        self.trailing = Some(handle);
    }

    /// Update the curve while the user drags a new control point.
    fn update_trailing(&mut self, handle: DPoint) {
        if self.points.len() > 1 {
            let len = self.points.len();
            assert!(self.points[len - 1].typ != PointType::OffCurve);
            assert!(self.points[len - 2].typ == PointType::OffCurve);
            let on_curve_pt = self.points[len - 1].point;
            let new_p = on_curve_pt.to_raw() - (handle.to_raw() - on_curve_pt.to_raw());
            self.points_mut()[len - 2].point = DPoint::from_raw(new_p);
        }
        self.trailing = Some(handle);
    }

    // in an open path, the first point is essentially a `move_to` command.
    // 'closing' the path means moving this path to the end of the list.
    fn close(&mut self) -> PointId {
        assert!(!self.closed);
        self.points_mut().rotate_left(1);
        self.closed = true;
        self.points.last().unwrap().id
    }

    #[inline]
    fn prev_idx(&self, idx: usize) -> usize {
        if idx == 0 {
            self.points.len() - 1
        } else {
            idx - 1
        }
    }

    #[inline]
    fn next_idx(&self, idx: usize) -> usize {
        (idx + 1) % self.points.len()
    }

    fn idx_for_point(&self, point: PointId) -> Option<usize> {
        self.points.iter().position(|p| p.id == point)
    }

    pub(crate) fn prev_point(&self, point: PointId) -> PathPoint {
        assert!(point.path == self.id);
        let idx = self.idx_for_point(point).expect("bad input to prev_point");
        let idx = self.prev_idx(idx);
        self.points[idx]
    }

    pub(crate) fn next_point(&self, point: PointId) -> PathPoint {
        assert!(point.path == self.id);
        let idx = self.idx_for_point(point).expect("bad input to next_point");
        let idx = self.next_idx(idx);
        self.points[idx]
    }
}

impl Data for Path {
    fn same(&self, other: &Self) -> bool {
        self.points.same(&other.points)
            && self.closed.same(&other.closed)
            && self.trailing == other.trailing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_point(x: f64, y: f64) -> PathPoint {
        PathPoint {
            point: DPoint { x, y },
            id: PointId { path: 0, point: 0 },
            typ: PointType::OnCurve,
        }
    }

    fn vport(offset: impl Into<Vec2>, zoom: f64) -> ViewPort {
        ViewPort {
            offset: offset.into(),
            zoom,
        }
    }

    #[test]
    fn screen_dist() {
        let point = Point::new(200., 200.);
        let ppoint = path_point(200., 200.);
        let vp = vport((200., 0.), 1.);
        assert_eq!(ppoint.screen_dist(vp, point), 200.);
        let vp = vport((0., 0.), 2.);
        assert_eq!(ppoint.screen_dist(vp, point), point.to_vec2().hypot());
        let vp = vport((0., 200.), 2.);
        assert_eq!(ppoint.screen_dist(vp, point), 200.);
    }
}
