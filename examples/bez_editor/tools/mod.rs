use std::any::Any;
use std::fmt::Debug;
use std::mem;

use super::{Contents, MIN_POINT_DISTANCE};
use druid::kurbo::{Point, Rect};
use druid::{Data, KeyEvent, MouseEvent};

mod pen;
mod select;

pub use pen::Pen;
pub use select::Select;

#[derive(Debug, Clone)]
enum MouseState {
    /// No mouse buttons are active.
    Up(MouseEvent),
    /// A mouse button has been pressed.
    Down(MouseEvent),
    /// The mouse has been moved some threshold distance with a button pressed.
    Drag {
        start: MouseEvent,
        current: MouseEvent,
    },
    /// A state only used as a placeholder during event handling.
    #[doc(hidden)]
    Transition,
}

#[derive(Debug, Clone)]
pub struct Mouse {
    state: MouseState,
}

impl Mouse {
    pub fn new() -> Mouse {
        use druid::{KeyModifiers, MouseButton};

        Mouse {
            state: MouseState::Up(MouseEvent {
                pos: Point::ZERO,
                mods: KeyModifiers::default(),
                count: 0,
                button: MouseButton::Left,
            }),
        }
    }

    pub fn pos(&self) -> Point {
        match &self.state {
            MouseState::Up(e) => e.pos,
            MouseState::Down(e) => e.pos,
            MouseState::Drag { current, .. } => current.pos,
            _ => panic!("transition is not an actual state :/"),
        }
    }
}

pub struct Drag<'a> {
    start: &'a MouseEvent,
    prev: &'a MouseEvent,
    current: &'a MouseEvent,
}

impl<'a> Drag<'a> {
    fn new(start: &'a MouseEvent, prev: &'a MouseEvent, current: &'a MouseEvent) -> Drag<'a> {
        Drag {
            start,
            prev,
            current,
        }
    }
}

impl Mouse {
    pub fn mouse_moved<T>(
        &mut self,
        data: &mut T,
        event: MouseEvent,
        delegate: &mut dyn MouseDelegate<T>,
    ) -> bool {
        let prev_state = mem::replace(&mut self.state, MouseState::Transition);
        let (next_state, handled) = match prev_state {
            MouseState::Up(_) => {
                let handled = delegate.mouse_moved(data, &event);
                (MouseState::Up(event), handled)
            }
            MouseState::Down(prev) => {
                if prev.pos.distance(event.pos) > MIN_POINT_DISTANCE {
                    let drag = Drag::new(&prev, &prev, &event);
                    let handled = if prev.button.is_left() {
                        delegate.left_drag_began(data, drag)
                    } else if prev.button.is_right() {
                        delegate.right_drag_began(data, drag)
                    } else {
                        delegate.other_drag_began(data, drag)
                    };
                    (
                        MouseState::Drag {
                            start: prev,
                            current: event,
                        },
                        handled,
                    )
                } else {
                    (MouseState::Down(prev), false)
                }
            }
            MouseState::Drag { start, current } => {
                let drag = Drag::new(&start, &current, &event);
                let handled = if start.button.is_left() {
                    delegate.left_drag_changed(data, drag)
                } else if start.button.is_right() {
                    delegate.right_drag_changed(data, drag)
                } else {
                    delegate.other_drag_changed(data, drag)
                };
                (
                    MouseState::Drag {
                        start,
                        current: event,
                    },
                    handled,
                )
            }
            MouseState::Transition => panic!("ahhhhhhh"),
        };
        self.state = next_state;
        handled
    }

    pub fn mouse_down<T>(
        &mut self,
        data: &mut T,
        event: MouseEvent,
        delegate: &mut dyn MouseDelegate<T>,
    ) -> bool {
        let prev_state = mem::replace(&mut self.state, MouseState::Transition);
        let (new_state, handled) = match prev_state {
            MouseState::Up(_) => {
                let handled = if event.button.is_left() {
                    delegate.left_down(data, &event)
                } else if event.button.is_right() {
                    delegate.right_down(data, &event)
                } else {
                    delegate.other_down(data, &event)
                };
                (MouseState::Down(event), handled)
            }
            MouseState::Down(prev) => {
                assert!(prev.button != event.button);
                // if a second button is pressed while we're handling an event
                // we just ignore it. At some point we could consider an event for this.
                (MouseState::Down(prev), false)
            }
            MouseState::Drag { start, .. } => {
                assert!(start.button != event.button);
                (
                    MouseState::Drag {
                        start,
                        current: event,
                    },
                    false,
                )
            }
            MouseState::Transition => panic!("ahhhhhhh"),
        };
        self.state = new_state;
        handled
    }

    pub fn mouse_up<T>(
        &mut self,
        data: &mut T,
        event: MouseEvent,
        delegate: &mut dyn MouseDelegate<T>,
    ) -> bool {
        let prev_state = mem::replace(&mut self.state, MouseState::Transition);
        let (new_state, handled) = match prev_state {
            MouseState::Up(_) => (MouseState::Up(event), false),
            MouseState::Down(prev) => {
                if event.button == prev.button {
                    let handled = if prev.button.is_left() {
                        delegate.left_up(data, &event) | delegate.left_click(data, &event)
                    } else if prev.button.is_right() {
                        delegate.right_up(data, &event) | delegate.right_click(data, &event)
                    } else {
                        delegate.other_up(data, &event) | delegate.other_click(data, &event)
                    };
                    (MouseState::Up(event), handled)
                } else {
                    (MouseState::Down(prev), false)
                }
            }
            MouseState::Drag { start, current } => {
                if event.button == start.button {
                    let drag = Drag {
                        start: &start,
                        current: &event,
                        prev: &current,
                    };
                    let handled = if start.button.is_left() {
                        delegate.left_up(data, &event) | delegate.left_drag_ended(data, drag)
                    } else if start.button.is_right() {
                        delegate.left_up(data, &event) | delegate.right_drag_ended(data, drag)
                    } else {
                        delegate.left_up(data, &event) | delegate.other_drag_ended(data, drag)
                    };
                    (MouseState::Up(event), handled)
                } else {
                    (MouseState::Drag { start, current }, false)
                }
            }
            MouseState::Transition => panic!("ahhhhhhh"),
        };
        self.state = new_state;
        handled
    }

    fn cancel<T>(&mut self, data: &mut T, delegate: &mut dyn MouseDelegate<T>) {
        let prev_state = mem::replace(&mut self.state, MouseState::Transition);
        let last_event = match prev_state {
            MouseState::Down(event) => event,
            MouseState::Up(event) => event,
            MouseState::Drag { current, .. } => current,
            MouseState::Transition => panic!("ahhhhhhh"),
        };
        delegate.cancel(data);
        self.state = MouseState::Up(last_event);
    }
}

pub trait MouseDelegate<T> {
    #[allow(unused)]
    fn mouse_moved(&mut self, _data: &mut T, _event: &MouseEvent) -> bool {
        false
    }

    #[allow(unused)]
    fn left_down(&mut self, _data: &mut T, _event: &MouseEvent) -> bool {
        false
    }
    #[allow(unused)]
    fn left_up(&mut self, _data: &mut T, _event: &MouseEvent) -> bool {
        false
    }
    #[allow(unused)]
    fn left_click(&mut self, _data: &mut T, _event: &MouseEvent) -> bool {
        false
    }

    #[allow(unused)]
    fn left_drag_began(&mut self, _data: &mut T, _drag: Drag) -> bool {
        false
    }
    #[allow(unused)]
    fn left_drag_changed(&mut self, _data: &mut T, _drag: Drag) -> bool {
        false
    }
    #[allow(unused)]
    fn left_drag_ended(&mut self, _data: &mut T, _drag: Drag) -> bool {
        false
    }

    #[allow(unused)]
    fn right_down(&mut self, _data: &mut T, _event: &MouseEvent) -> bool {
        false
    }
    #[allow(unused)]
    fn right_up(&mut self, _data: &mut T, _event: &MouseEvent) -> bool {
        false
    }
    #[allow(unused)]
    fn right_click(&mut self, _data: &mut T, _event: &MouseEvent) -> bool {
        false
    }

    #[allow(unused)]
    fn right_drag_began(&mut self, _data: &mut T, _drag: Drag) -> bool {
        false
    }
    #[allow(unused)]
    fn right_drag_changed(&mut self, _data: &mut T, _drag: Drag) -> bool {
        false
    }
    #[allow(unused)]
    fn right_drag_ended(&mut self, _data: &mut T, _drag: Drag) -> bool {
        false
    }
    #[allow(unused)]
    fn other_down(&mut self, _data: &mut T, _event: &MouseEvent) -> bool {
        false
    }
    #[allow(unused)]
    fn other_up(&mut self, _data: &mut T, _event: &MouseEvent) -> bool {
        false
    }
    #[allow(unused)]
    fn other_click(&mut self, _data: &mut T, _event: &MouseEvent) -> bool {
        false
    }

    #[allow(unused)]
    fn other_drag_began(&mut self, _data: &mut T, _drag: Drag) -> bool {
        false
    }
    #[allow(unused)]
    fn other_drag_changed(&mut self, _data: &mut T, _drag: Drag) -> bool {
        false
    }
    #[allow(unused)]
    fn other_drag_ended(&mut self, _data: &mut T, _drag: Drag) -> bool {
        false
    }

    #[allow(unused)]
    fn cancel(&mut self, data: &mut T);
}

/// A trait for editor tools (selection, pen, etc). More concretely, this abstracts
/// away different sets of mouse and keyboard handling behaviour.
pub(crate) trait Tool: Debug + Any + MouseDelegate<Contents> {
    /// Called when the tool should process some key_down event. The tool should
    /// modify `data` as necessary, and return `true` if the event is handled.
    fn key_down(&mut self, _data: &mut Contents, _event: &KeyEvent) -> bool {
        false
    }

    /// The current rectangular selection, if this is the selection tool, and
    /// whether or not the shift key is downpick 5ca86ff fixup corner point drawing.
    fn selection_rect(&self) -> Option<Rect> {
        None
    }

    fn boxed_clone(&self) -> Box<dyn Tool>;
    //TODO: this doesn't work; remove me, probably make tool an `enum`.
    fn same_impl(&self, _other: &dyn Any) -> bool {
        false
    }
    fn name(&self) -> &str;
}

impl Clone for Box<dyn Tool> {
    fn clone(&self) -> Self {
        self.boxed_clone()
    }
}

// ugh
use std::ops::DerefMut;
impl MouseDelegate<Contents> for Box<dyn Tool> {
    fn mouse_moved(&mut self, data: &mut Contents, event: &MouseEvent) -> bool {
        self.deref_mut().mouse_moved(data, event)
    }
    fn left_down(&mut self, data: &mut Contents, event: &MouseEvent) -> bool {
        self.deref_mut().left_down(data, event)
    }
    fn left_up(&mut self, data: &mut Contents, event: &MouseEvent) -> bool {
        self.deref_mut().left_up(data, event)
    }
    fn left_click(&mut self, data: &mut Contents, event: &MouseEvent) -> bool {
        self.deref_mut().left_click(data, event)
    }
    fn left_drag_began(&mut self, data: &mut Contents, drag: Drag) -> bool {
        self.deref_mut().left_drag_began(data, drag)
    }
    fn left_drag_changed(&mut self, data: &mut Contents, drag: Drag) -> bool {
        self.deref_mut().left_drag_changed(data, drag)
    }
    fn left_drag_ended(&mut self, data: &mut Contents, drag: Drag) -> bool {
        self.deref_mut().left_drag_ended(data, drag)
    }
    fn right_down(&mut self, data: &mut Contents, event: &MouseEvent) -> bool {
        self.deref_mut().right_down(data, event)
    }
    fn right_up(&mut self, data: &mut Contents, event: &MouseEvent) -> bool {
        self.deref_mut().right_up(data, event)
    }
    fn right_click(&mut self, data: &mut Contents, event: &MouseEvent) -> bool {
        self.deref_mut().right_click(data, event)
    }
    fn right_drag_began(&mut self, data: &mut Contents, drag: Drag) -> bool {
        self.deref_mut().right_drag_began(data, drag)
    }
    fn right_drag_changed(&mut self, data: &mut Contents, drag: Drag) -> bool {
        self.deref_mut().right_drag_changed(data, drag)
    }
    fn right_drag_ended(&mut self, data: &mut Contents, drag: Drag) -> bool {
        self.deref_mut().right_drag_ended(data, drag)
    }
    fn other_down(&mut self, data: &mut Contents, event: &MouseEvent) -> bool {
        self.deref_mut().other_down(data, event)
    }
    fn other_up(&mut self, data: &mut Contents, event: &MouseEvent) -> bool {
        self.deref_mut().other_up(data, event)
    }
    fn other_click(&mut self, data: &mut Contents, event: &MouseEvent) -> bool {
        self.deref_mut().other_click(data, event)
    }
    fn other_drag_began(&mut self, data: &mut Contents, drag: Drag) -> bool {
        self.deref_mut().other_drag_began(data, drag)
    }
    fn other_drag_changed(&mut self, data: &mut Contents, drag: Drag) -> bool {
        self.deref_mut().other_drag_changed(data, drag)
    }
    fn other_drag_ended(&mut self, data: &mut Contents, drag: Drag) -> bool {
        self.deref_mut().other_drag_ended(data, drag)
    }
    fn cancel(&mut self, data: &mut Contents) {
        self.deref_mut().cancel(data)
    }
}

impl Data for Box<dyn Tool> {
    fn same(&self, other: &Box<dyn Tool>) -> bool {
        self.same_impl(other)
    }
}
