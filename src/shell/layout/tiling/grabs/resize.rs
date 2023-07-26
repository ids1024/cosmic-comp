// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    backend::render::cursor::{CursorShape, CursorState},
    shell::{focus::target::PointerFocusTarget, layout::Orientation},
    utils::prelude::*,
};
use id_tree::NodeId;
use smithay::{
    backend::input::ButtonState,
    input::{
        pointer::{
            AxisFrame, ButtonEvent, Focus, GrabStartData as PointerGrabStartData, MotionEvent,
            PointerGrab, PointerInnerHandle, PointerTarget, RelativeMotionEvent,
        },
        Seat,
    },
    output::WeakOutput,
    utils::{IsAlive, Logical, Point},
};

use super::super::{Data, TilingLayout};

#[derive(Debug, Clone, PartialEq)]
pub struct ResizeForkTarget {
    pub node: NodeId,
    pub output: WeakOutput,
    pub left_up_idx: usize,
    pub orientation: Orientation,
}

impl IsAlive for ResizeForkTarget {
    fn alive(&self) -> bool {
        self.output.upgrade().is_some()
    }
}

impl PointerTarget<State> for ResizeForkTarget {
    fn enter(&self, seat: &Seat<State>, _data: &mut State, _event: &MotionEvent) {
        let user_data = seat.user_data();
        let cursor_state = user_data.get::<CursorState>().unwrap();
        cursor_state.set_shape(match self.orientation {
            Orientation::Horizontal => CursorShape::RowResize,
            Orientation::Vertical => CursorShape::ColResize,
        });
    }

    fn leave(
        &self,
        seat: &Seat<State>,
        _data: &mut State,
        _serial: smithay::utils::Serial,
        _time: u32,
    ) {
        let user_data = seat.user_data();
        let cursor_state = user_data.get::<CursorState>().unwrap();
        cursor_state.set_shape(CursorShape::Default)
    }

    fn button(&self, seat: &Seat<State>, data: &mut State, event: &ButtonEvent) {
        if event.button == 0x110 && event.state == ButtonState::Pressed {
            let seat = seat.clone();
            let node = self.node.clone();
            let output = self.output.clone();
            let left_up_idx = self.left_up_idx;
            let orientation = self.orientation;
            let serial = event.serial;
            let button = event.button;
            data.common.event_loop_handle.insert_idle(move |data| {
                let pointer = seat.get_pointer().unwrap();
                let location = pointer.current_location();
                pointer.set_grab(
                    &mut data.state,
                    ResizeForkGrab {
                        start_data: PointerGrabStartData {
                            focus: None,
                            button,
                            location,
                        },
                        last_loc: location,
                        node,
                        output,
                        left_up_idx,
                        orientation,
                    },
                    serial,
                    Focus::Clear,
                )
            });
        }
    }

    fn motion(&self, _seat: &Seat<State>, _data: &mut State, _event: &MotionEvent) {}
    fn relative_motion(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _event: &RelativeMotionEvent,
    ) {
    }
    fn axis(&self, _seat: &Seat<State>, _data: &mut State, _frame: AxisFrame) {}
}

pub struct ResizeForkGrab {
    start_data: PointerGrabStartData<State>,
    last_loc: Point<f64, Logical>,
    node: NodeId,
    output: WeakOutput,
    left_up_idx: usize,
    orientation: Orientation,
}

impl PointerGrab<State> for ResizeForkGrab {
    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(PointerFocusTarget, Point<i32, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);

        let delta = event.location - self.last_loc;

        if let Some(output) = self.output.upgrade() {
            let tiling_layer = &mut data.common.shell.active_space_mut(&output).tiling_layer;
            if let Some(queue) = tiling_layer.queues.get_mut(&output) {
                let tree = &mut queue.trees.back_mut().unwrap().0;
                if tree.get(&self.node).is_ok() {
                    let delta = match self.orientation {
                        Orientation::Vertical => delta.x,
                        Orientation::Horizontal => delta.y,
                    }
                    .round() as i32;

                    // check that we are still alive
                    let mut iter = tree
                        .children_ids(&self.node)
                        .unwrap()
                        .skip(self.left_up_idx);
                    let first_elem = iter.next();
                    let second_elem = iter.next();
                    if first_elem.is_none() || second_elem.is_none() {
                        return handle.unset_grab(data, event.serial, event.time);
                    };

                    match tree.get_mut(&self.node).unwrap().data_mut() {
                        Data::Group {
                            sizes, orientation, ..
                        } => {
                            if sizes[self.left_up_idx] + sizes[self.left_up_idx + 1]
                                < match orientation {
                                    Orientation::Vertical => 720,
                                    Orientation::Horizontal => 480,
                                }
                            {
                                return;
                            };

                            let old_size = sizes[self.left_up_idx];
                            sizes[self.left_up_idx] = (old_size + delta).max(
                                if self.orientation == Orientation::Vertical {
                                    360
                                } else {
                                    240
                                },
                            );
                            let diff = old_size - sizes[self.left_up_idx];
                            let next_size = sizes[self.left_up_idx + 1] + diff;
                            sizes[self.left_up_idx + 1] =
                                next_size.max(if self.orientation == Orientation::Vertical {
                                    360
                                } else {
                                    240
                                });
                            let next_diff = next_size - sizes[self.left_up_idx + 1];
                            sizes[self.left_up_idx] += next_diff;
                        }
                        _ => unreachable!(),
                    }

                    self.last_loc = event.location;
                    let blocker = TilingLayout::update_positions(&output, tree, tiling_layer.gaps);
                    tiling_layer.pending_blockers.extend(blocker);
                } else {
                    handle.unset_grab(data, event.serial, event.time);
                }
            }
        }
    }

    fn relative_motion(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(PointerFocusTarget, Point<i32, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.relative_motion(state, None, event);
    }

    fn button(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            // No more buttons are pressed, release the grab.
            handle.unset_grab(data, event.serial, event.time);
        }
    }

    fn axis(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        details: AxisFrame,
    ) {
        handle.axis(data, details)
    }

    fn start_data(&self) -> &PointerGrabStartData<State> {
        &self.start_data
    }
}