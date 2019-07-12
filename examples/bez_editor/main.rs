// Copyright 2018 The xi-editor Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A simple bezier path editor.

use druid::kurbo::{Point, Rect, Size};
use druid::piet::{Color, RenderContext};
use druid::shell::window::Cursor;
use druid::shell::{runloop, WindowBuilder};
use std::sync::Arc;

use druid::{
    Action, BaseState, BoxConstraints, Data, Env, Event, EventCtx, KeyCode, LayoutCtx, PaintCtx,
    UiMain, UiState, UpdateCtx, Widget, WidgetPod,
};

mod draw;
mod path;
mod pen;
mod toolbar;

use draw::draw_paths;
use path::{Path, PointId};
use pen::Pen;
use toolbar::{Toolbar, ToolbarState};

const BG_COLOR: Color = Color::rgb24(0xfb_fb_fb);
const TOOLBAR_POSITION: Point = Point::new(8., 8.);

pub(crate) const MIN_POINT_DISTANCE: f64 = 3.0;

struct Canvas {
    toolbar: WidgetPod<ToolbarState, Toolbar>,
}

impl Canvas {
    fn new() -> Self {
        Canvas {
            toolbar: WidgetPod::new(Toolbar::default()),
        }
    }
}

#[derive(Debug, Clone)]
struct CanvasState {
    tool: Pen,
    /// The paths in the canvas
    contents: Contents,
    toolbar: ToolbarState,
}

impl CanvasState {
    fn new() -> Self {
        CanvasState {
            tool: Pen::new(),
            contents: Contents::default(),
            toolbar: ToolbarState::basic(),
        }
    }

    fn remove_top_path(&mut self) {
        Arc::make_mut(&mut self.contents.paths).pop();
        Arc::make_mut(&mut self.contents.selection).clear();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SelectionId {
    path_idx: usize,
    point_id: PointId,
}

impl SelectionId {
    fn new(path_idx: usize, point_id: PointId) -> SelectionId {
        SelectionId { path_idx, point_id }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Contents {
    next_path_id: usize,
    paths: Arc<Vec<Path>>,
    /// Selected points, including the path index and the point id.
    selection: Arc<Vec<SelectionId>>,
}

impl Contents {
    pub(crate) fn paths_mut(&mut self) -> &mut Vec<Path> {
        Arc::make_mut(&mut self.paths)
    }

    pub(crate) fn selection_mut(&mut self) -> &mut Vec<SelectionId> {
        Arc::make_mut(&mut self.selection)
    }

    /// Return the index of the path that is currently drawing. To be currently
    /// drawing, there must be a single currently selected point.
    fn active_path_idx(&self) -> Option<usize> {
        if self.selection.len() == 1 {
            Some(self.selection[0].path_idx)
        } else {
            None
        }
    }

    pub(crate) fn active_path_mut(&mut self) -> Option<&mut Path> {
        match self.active_path_idx() {
            Some(idx) => self.paths_mut().get_mut(idx),
            None => None,
        }
    }

    pub(crate) fn active_path(&self) -> Option<&Path> {
        match self.active_path_idx() {
            Some(idx) => self.paths.get(idx),
            None => None,
        }
    }

    pub(crate) fn new_path(&mut self, start: Point) {
        let path = Path::new(start);
        let path_id = self.paths.len();
        let point_id = path.last_point_id();

        self.paths_mut().push(path);
        self.selection_mut().clear();
        self.selection_mut()
            .push(SelectionId::new(path_id, point_id));
    }

    pub(crate) fn add_point(&mut self, point: Point) {
        if self.active_path_idx().is_none() {
            self.new_path(point);
        } else {
            let new_point = self.active_path_mut().unwrap().append_point(point);
            self.selection_mut()[0].point_id = new_point;
        }
        //eprintln!("SEL: {:?}", self.selection.first());
    }

    pub(crate) fn update_for_drag(&mut self, _start: Point, end: Point) {
        self.active_path_mut().unwrap().update_for_drag(end);
        //eprintln!("SEL: {:?}", self.selection.first());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Mouse {
    Down(Point),
    Drag { start: Point, current: Point },
    Up(Point),
}

/// A trait for editor tools (selection, pen, etc). More concretely, this abstracts
/// away different sets of mouse and keyboard handling behaviour.
pub(crate) trait Tool {
    fn event(&mut self, data: &mut Contents, event: &Event) -> bool;
}

// It should be able to get this from a derive macro.
impl Data for CanvasState {
    fn same(&self, other: &Self) -> bool {
        self.contents.same(&other.contents)
            && self.toolbar.same(&other.toolbar)
            && self.tool == other.tool
    }
}

impl Data for Contents {
    fn same(&self, other: &Self) -> bool {
        self.paths.same(&other.paths) && self.selection.same(&other.selection)
    }
}

impl Widget<CanvasState> for Canvas {
    fn paint(
        &mut self,
        paint_ctx: &mut PaintCtx,
        _base: &BaseState,
        data: &CanvasState,
        _env: &Env,
    ) {
        paint_ctx.render_ctx.clear(BG_COLOR);
        draw_paths(&data.contents.paths, &data.contents.selection, paint_ctx);
        self.toolbar
            .paint_with_offset(paint_ctx, &data.toolbar, _env);
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        data: &CanvasState,
        env: &Env,
    ) -> Size {
        let toolbar_size = self.toolbar.layout(ctx, bc, &data.toolbar, env);
        self.toolbar
            .set_layout_rect(Rect::from_origin_size(TOOLBAR_POSITION, toolbar_size));
        bc.max()
    }

    fn event(
        &mut self,
        event: &Event,
        ctx: &mut EventCtx,
        data: &mut CanvasState,
        _env: &Env,
    ) -> Option<Action> {
        // first check for top-level commands
        match event {
            Event::KeyUp(key) if key.key_code == KeyCode::Escape => {
                data.remove_top_path();
                ctx.set_handled();
            }
            Event::KeyUp(key) if data.toolbar.idx_for_key(key).is_some() => {
                let idx = data.toolbar.idx_for_key(key).unwrap();
                data.toolbar.set_selected(idx);
                ctx.set_handled();
            }
            other => {
                self.toolbar.event(other, ctx, &mut data.toolbar, _env);
            }
        }

        // then pass the event to the active tool
        let CanvasState { tool, contents, .. } = data;
        if ctx.is_handled() | tool.event(contents, event) {
            ctx.invalidate();
        }
        None
    }

    fn update(
        &mut self,
        ctx: &mut UpdateCtx,
        old: Option<&CanvasState>,
        new: &CanvasState,
        _env: &Env,
    ) {
        // update the mouse icon if the active tool has changed
        let old = match old {
            Some(old) => old,
            None => return,
        };

        if old.toolbar.selected_idx() != new.toolbar.selected_idx() {
            match new.toolbar.selected_item().name.as_str() {
                "select" => ctx.window().set_cursor(&Cursor::Arrow),
                "pen" => ctx.window().set_cursor(&Cursor::Crosshair),
                other => eprintln!("unknown tool '{}'", other),
            }
            ctx.invalidate();
        }
        self.toolbar.update(ctx, &new.toolbar, _env);
    }
}

fn main() {
    druid_shell::init();

    let mut run_loop = runloop::RunLoop::new();
    let mut builder = WindowBuilder::new();
    let state = CanvasState::new();
    let mut state = UiState::new(Canvas::new(), state);
    state.set_active(true);
    builder.set_title("Paths");
    builder.set_handler(Box::new(UiMain::new(state)));
    let window = builder.build().unwrap();
    window.show();
    run_loop.run();
}