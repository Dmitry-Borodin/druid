//! Drawing algorithms and helpers

use std::collections::BTreeSet;

use super::path::{Path, PointId, PointType};
use super::{Tool, ViewPort};
use druid::kurbo::{Affine, BezPath, Circle, CubicBez, Line, PathSeg, Point, Rect, Vec2};
use druid::piet::{Color, FillRule::NonZero, Piet, RenderContext};
use druid::PaintCtx;

const PATH_COLOR: Color = Color::rgb24(0x00_00_00);
const SELECTION_RECT_BG_COLOR: Color = Color::rgba32(0xDD_DD_DD_55);
const SELECTION_RECT_STROKE_COLOR: Color = Color::rgb24(0x53_8B_BB);
const SMOOTH_POINT_COLOR: Color = Color::rgb24(0x_41_8E_22);
const CORNER_POINT_COLOR: Color = Color::rgb24(0x0b_2b_db);
const OFF_CURVE_POINT_COLOR: Color = Color::rgb24(0xbb_bb_bb);
const OFF_CURVE_HANDLE_COLOR: Color = Color::rgb24(0xbb_bb_bb);
const DIRECTION_ARROW_COLOR: Color = Color::rgba32(0x00_00_00_44);

const SMOOTH_RADIUS: f64 = 3.5;
const SMOOTH_SELECTED_RADIUS: f64 = 4.;
const OFF_CURVE_RADIUS: f64 = 2.;
const OFF_CURVE_SELECTED_RADIUS: f64 = 2.5;

/// A context for drawing that maps between screen space and design space.
struct DrawCtx<'a, 'b: 'a> {
    ctx: &'a mut Piet<'b>,
    space: ViewPort,
}

impl<'a, 'b> std::ops::Deref for DrawCtx<'a, 'b> {
    type Target = Piet<'b>;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

impl<'a, 'b> std::ops::DerefMut for DrawCtx<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ctx
    }
}

impl<'a, 'b: 'a> DrawCtx<'a, 'b> {
    fn new(ctx: &'a mut Piet<'b>, space: ViewPort) -> Self {
        DrawCtx { ctx, space }
    }

    fn draw_origin(&mut self) {
        let brush = self.solid_brush(Color::rgb24(0x_FF_AA_AA));
        let brush1 = self.solid_brush(Color::rgb24(0x_00_00_FF));
        let brush2 = self.solid_brush(Color::rgb24(0x_AA_AA_AA));
        for i in -100..=100_i32 {
            let brush = match i.abs() {
                0 => &brush1,
                x if x % 3 == 0 => &brush,
                _ => &brush2,
            };
            let c = i as f64 * 20.0;
            let xmax = self.space.to_screen((c, 5000.));
            let xmin = self.space.to_screen((c, -5000.));
            let ymax = self.space.to_screen((5000., c));
            let ymin = self.space.to_screen((-5000., c));

            if i.abs() % 3 == 0 || self.space.zoom >= 1.0 {
                self.stroke(Line::new(xmin, xmax), brush, 1.0, None);
                self.stroke(Line::new(ymin, ymax), brush, 1.0, None);
            }
        }
    }

    fn draw_path(&mut self, bez: &BezPath) {
        let path_brush = self.solid_brush(PATH_COLOR);
        self.stroke(bez, &path_brush, 1.0, None);
    }

    fn draw_control_point_lines(&mut self, path: &Path) {
        let mut prev_point = path.start_point().to_screen(self.space);
        let mut idx = 0;
        while idx < path.points().len() {
            match path.points()[idx] {
                p if p.is_on_curve() => prev_point = p.to_screen(self.space),
                p => {
                    self.draw_control_handle(prev_point, p.to_screen(self.space));
                    let p1 = path.points()[idx + 1].to_screen(self.space);
                    let p2 = path.points()[idx + 2].to_screen(self.space);
                    self.draw_control_handle(p1, p2);
                    idx += 2;
                    prev_point = p2;
                }
            }
            idx += 1;
        }

        if let Some(trailing) = path.trailing() {
            if path.should_draw_trailing() {
                self.draw_control_handle(prev_point, trailing.to_screen(self.space));
            }
        }
    }

    fn draw_control_handle(&mut self, p1: Point, p2: Point) {
        let brush = self.solid_brush(OFF_CURVE_HANDLE_COLOR);
        let l = Line::new(p1, p2);
        self.stroke(l, &brush, 1.0, None);
    }

    fn draw_point(&mut self, point: PointStyle) {
        let PointStyle {
            style,
            point,
            selected,
        } = point;
        match style {
            Style::Open(seg) => self.draw_open_path_terminal(&seg, selected),
            Style::Close(seg) => self.draw_open_path_terminal(&seg, selected),
            Style::OffCurve => self.draw_off_curve_point(point, selected),
            Style::Smooth => self.draw_smooth_point(point, selected),
            Style::Tangent => self.draw_smooth_point(point, selected),
            Style::Corner => self.draw_corner_point(point, selected),
        }
    }

    fn draw_open_path_terminal(&mut self, seg: &PathSeg, selected: bool) {
        let cap = cap_line(seg.to_cubic(), 12.);
        let brush = self.solid_brush(OFF_CURVE_HANDLE_COLOR);
        if selected {
            self.stroke(cap, &brush, 3.0, None);
        } else {
            self.stroke(cap, &brush, 2.0, None);
        }
    }

    fn draw_smooth_point(&mut self, p: Point, selected: bool) {
        let radius = if selected {
            SMOOTH_SELECTED_RADIUS
        } else {
            SMOOTH_RADIUS
        };
        let brush = self.solid_brush(SMOOTH_POINT_COLOR);
        let circ = Circle::new(p, radius);
        if selected {
            self.fill(circ, &brush, NonZero);
        } else {
            self.stroke(circ, &brush, 1.0, None);
        }
    }

    fn draw_corner_point(&mut self, p: Point, selected: bool) {
        let radius = if selected {
            SMOOTH_SELECTED_RADIUS
        } else {
            SMOOTH_RADIUS
        };
        let brush = self.solid_brush(CORNER_POINT_COLOR);
        let rect = Rect::new(p.x - radius, p.y - radius, p.x + radius, p.y + radius);
        if selected {
            self.fill(rect, &brush, NonZero);
        } else {
            self.stroke(rect, &brush, 1.0, None);
        }
    }

    fn draw_off_curve_point(&mut self, p: Point, selected: bool) {
        let radius = if selected {
            OFF_CURVE_SELECTED_RADIUS
        } else {
            OFF_CURVE_RADIUS
        };
        let brush = self.solid_brush(OFF_CURVE_POINT_COLOR);
        let circ = Circle::new(p, radius);
        if selected {
            self.fill(circ, &brush, NonZero);
        } else {
            self.stroke(circ, &brush, 1.0, None);
        }
    }

    fn draw_selection_rect(&mut self, rect: Rect) {
        let bg_brush = self.solid_brush(SELECTION_RECT_BG_COLOR);
        let stroke_brush = self.solid_brush(SELECTION_RECT_STROKE_COLOR);
        self.fill(rect, &bg_brush, NonZero);
        self.stroke(rect, &stroke_brush, 1.0, None);
    }

    fn draw_direction_indicator(&mut self, path: &BezPath) {
        let first_seg = match path.segments().next().as_ref().map(PathSeg::to_cubic) {
            None => return,
            Some(cubic) => cubic,
        };

        let tangent = tangent_vector(0.05, first_seg).normalize();
        let angle = Vec2::new(tangent.y, -tangent.x);
        let rotate = Affine::rotate(angle.atan2());
        let translate = Affine::translate(first_seg.p0.to_vec2() + tangent * 4.0);
        let mut arrow = make_arrow();
        arrow.apply_affine(rotate);
        arrow.apply_affine(translate);
        let brush = self.solid_brush(DIRECTION_ARROW_COLOR);
        self.fill(arrow, &brush, NonZero);
    }
}

struct PointStyle {
    point: Point,
    style: Style,
    selected: bool,
}

enum Style {
    Open(PathSeg),
    Close(PathSeg),
    Corner,
    Smooth,
    Tangent,
    OffCurve,
}

struct PointIter<'a> {
    idx: usize,
    vport: ViewPort,
    path: &'a Path,
    bez: &'a BezPath,
    sels: &'a BTreeSet<PointId>,
}

impl<'a> PointIter<'a> {
    fn new(path: &'a Path, vport: ViewPort, bez: &'a BezPath, sels: &'a BTreeSet<PointId>) -> Self {
        PointIter {
            idx: 0,
            vport,
            bez,
            path,
            sels,
        }
    }

    fn next_style(&self) -> Style {
        let len = self.path.points().len();
        if len == 1 {
            return Style::Corner;
        }

        let this = self.path.points()[self.idx];
        if this.is_on_curve() && !self.path.is_closed() {
            if self.idx == 0 {
                return Style::Open(self.bez.segments().next().unwrap());
            } else if self.idx == len - 1 {
                return Style::Close(self.bez.segments().last().unwrap().reverse());
            }
        }

        match this.typ {
            PointType::OnCurve => Style::Corner,
            PointType::OffCurve => Style::OffCurve,
            PointType::OnCurveSmooth => {
                let prev = self.path.prev_point(this.id);
                let next = self.path.next_point(this.id);
                match (prev.is_on_curve(), next.is_on_curve()) {
                    (false, false) => Style::Smooth,
                    (true, false) | (false, true) => Style::Tangent,
                    _ => unreachable!(),
                }
            }
        }
    }
}

impl<'a> std::iter::Iterator for PointIter<'a> {
    type Item = PointStyle;
    fn next(&mut self) -> Option<PointStyle> {
        let point = self.path.points().get(self.idx)?;
        let style = self.next_style();
        let selected = self.sels.contains(&point.id);
        let point = point.to_screen(self.vport);
        self.idx += 1;
        Some(PointStyle {
            point,
            style,
            selected,
        })
    }
}

pub(crate) fn draw_paths(
    paths: &[Path],
    sels: &BTreeSet<PointId>,
    tool: &dyn Tool,
    space: ViewPort,
    ctx: &mut PaintCtx,
    _mouse: Point,
) {
    let mut draw_ctx = DrawCtx::new(&mut ctx.render_ctx, space);
    draw_ctx.draw_origin();
    for path in paths {
        let bez = space.transform() * path.bezier().clone();
        draw_ctx.draw_path(&bez);
        draw_ctx.draw_control_point_lines(path);
        draw_ctx.draw_direction_indicator(&bez);

        for point in PointIter::new(path, space, &bez, sels) {
            draw_ctx.draw_point(point)
        }

        if let Some(pt) = path.trailing() {
            if path.should_draw_trailing() {
                draw_ctx.draw_off_curve_point(pt.to_screen(space), true);
            }
        }
    }

    if let Some(rect) = tool.selection_rect() {
        draw_ctx.draw_selection_rect(rect);
    }
}

/// Return the tangent of the cubic bezier `cb`, at time `t`, as a vector
/// relative to the path's start point.
fn tangent_vector(t: f64, cb: CubicBez) -> Vec2 {
    debug_assert!(t >= 0.0 && t <= 1.0);
    let CubicBez { p0, p1, p2, p3 } = cb;
    let one_minus_t = 1.0 - t;
    3.0 * one_minus_t.powi(2) * (p1 - p0)
        + 6.0 * t * one_minus_t * (p2 - p1)
        + 3.0 * t.powi(2) * (p3 - p2)
}

/// Create a line of length `len` perpendicular to the tangent of the cubic
/// bezier `cb`, centered on the bezier's start point.
fn cap_line(cb: CubicBez, len: f64) -> Line {
    let tan_vec = tangent_vector(0.01, cb);
    let end = cb.p0 + tan_vec;
    perp(cb.p0, end, len)
}

/// Create a line perpendicular to the line `(p1, p2)`, centered on `p1`.
fn perp(p0: Point, p1: Point, len: f64) -> Line {
    let perp_vec = Vec2::new(p0.y - p1.y, p1.x - p0.x);
    let norm_perp = perp_vec / perp_vec.hypot();
    let p2 = p0 + (len * -0.5) * norm_perp;
    let p3 = p0 + (len * 0.5) * norm_perp;
    Line::new(p2, p3)
}

fn make_arrow() -> BezPath {
    let mut bez = BezPath::new();
    //bez.move_to((-5., 0.));
    //bez.line_to((5., 0.));
    //bez.line_to((5., 11.));
    //bez.line_to((15., 11.));
    //bez.line_to((0., 32.));
    //bez.line_to((-15., 11.));
    //bez.line_to((-5., 11.));
    //bez.close_path();

    bez.move_to((0., 18.));
    bez.line_to((-12., 0.));
    bez.line_to((12., 0.));
    bez.close_path();
    bez
}
