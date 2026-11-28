//! Pointer capture on the first window's world plane.

use pythos_shared::{
    graph_window::{Rect, Scene},
    session_input_abi::{
        KEY_DIGIT0, KEY_ESCAPE, SESSION_INPUT_FLAG_GAP_BEFORE, SESSION_INPUT_KIND_KEY_DOWN,
        SESSION_INPUT_KIND_MOUSE_BUTTON_STATE, SESSION_INPUT_KIND_RELATIVE_MOTION,
        SessionInputEventV1,
    },
    world_session::HALF_WIDTH_MM,
};

const DEPTH_MM: i32 = 3000;
const EYE_MM: i32 = 1650;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Pose {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Pose {
    pub fn valid(self) -> bool {
        self.z == -DEPTH_MM
            && (-HALF_WIDTH_MM..=HALF_WIDTH_MM).contains(&self.x)
            && (-HALF_WIDTH_MM..=HALF_WIDTH_MM).contains(&self.y)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Drop {
    pub sequence: u64,
    pub pose: Pose,
}

pub(super) enum Update {
    None,
    Captured(u64),
    Dropped(Drop),
    Exit,
}

struct Drag {
    started_at: [i32; 2],
    offset: [i32; 2],
}

pub(super) struct Layout {
    view: [i32; 2],
    bounds: [i32; 2],
    title: Rect,
    pub position: [i32; 2],
    pub cursor: [i32; 2],
    left_down: bool,
    drag: Option<Drag>,
}

impl Layout {
    pub fn new(view: [i32; 2], scene: &Scene, saved: Option<Pose>) -> Self {
        let bounds = [
            view[0] - i32::from(scene.width),
            view[1] - i32::from(scene.height),
        ];
        let mut layout = Self {
            view,
            bounds,
            title: scene.title_bar,
            position: bounds.map(|extent| extent / 2),
            cursor: [0; 2],
            left_down: false,
            drag: None,
        };
        if let Some(saved) = saved {
            layout.position = layout.position_for(saved);
        }
        let Rect {
            x,
            y,
            width,
            height,
        } = layout.title;
        layout.cursor = [
            layout.position[0] + i32::from(x + width / 2),
            layout.position[1] + i32::from(y + height / 2),
        ];
        layout
    }

    pub fn pose(&self) -> Pose {
        let [width, height] = self.view;
        let focal = height * 4 / 5;
        let horizon = height * 44 / 100;
        Pose {
            x: round_ratio(self.position[0] - width / 2, DEPTH_MM, focal),
            y: EYE_MM + round_ratio(horizon - self.position[1], DEPTH_MM, focal),
            z: -DEPTH_MM,
        }
    }

    pub fn position_for(&self, pose: Pose) -> [i32; 2] {
        let [width, height] = self.view;
        let focal = height * 4 / 5;
        let horizon = height * 44 / 100;
        self.clamp_position([
            width / 2 + round_ratio(pose.x, focal, DEPTH_MM),
            horizon + round_ratio(EYE_MM - pose.y, focal, DEPTH_MM),
        ])
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn input(&mut self, event: SessionInputEventV1) -> Update {
        if event.flags & SESSION_INPUT_FLAG_GAP_BEFORE != 0 {
            self.cancel();
            self.left_down = false;
        }
        match (event.kind, event.value0) {
            (SESSION_INPUT_KIND_KEY_DOWN, key) if key == i32::from(KEY_DIGIT0) => Update::Exit,
            (SESSION_INPUT_KIND_KEY_DOWN, key) if key == i32::from(KEY_ESCAPE) => {
                self.cancel();
                Update::None
            }
            (SESSION_INPUT_KIND_RELATIVE_MOTION, dx) => {
                self.move_cursor(dx, event.value1);
                Update::None
            }
            (SESSION_INPUT_KIND_MOUSE_BUTTON_STATE, 1) if !self.left_down => {
                self.left_down = true;
                let offset = [
                    self.cursor[0] - self.position[0],
                    self.cursor[1] - self.position[1],
                ];
                let Rect {
                    x,
                    y,
                    width,
                    height,
                } = self.title;
                if (i32::from(x)..i32::from(x + width)).contains(&offset[0])
                    && (i32::from(y)..i32::from(y + height)).contains(&offset[1])
                {
                    self.drag = Some(Drag {
                        started_at: self.position,
                        offset,
                    });
                    return Update::Captured(event.sequence);
                }
                Update::None
            }
            (SESSION_INPUT_KIND_MOUSE_BUTTON_STATE, 0) => {
                self.left_down = false;
                if let Some(drag) = self.drag.take()
                    && drag.started_at != self.position
                {
                    return Update::Dropped(Drop {
                        sequence: event.sequence,
                        pose: self.pose(),
                    });
                }
                Update::None
            }
            _ => Update::None,
        }
    }

    fn clamp_position(&self, [x, y]: [i32; 2]) -> [i32; 2] {
        [x.clamp(0, self.bounds[0]), y.clamp(0, self.bounds[1])]
    }

    fn move_cursor(&mut self, dx: i32, dy: i32) {
        self.cursor = [
            self.cursor[0].saturating_add(dx).clamp(0, self.view[0] - 1),
            self.cursor[1].saturating_add(dy).clamp(0, self.view[1] - 1),
        ];
        if let Some(drag) = &self.drag {
            self.position = self.clamp_position([
                self.cursor[0] - drag.offset[0],
                self.cursor[1] - drag.offset[1],
            ]);
        }
    }

    fn cancel(&mut self) {
        if let Some(drag) = self.drag.take() {
            self.position = drag.started_at;
        }
    }
}

fn round_ratio(value: i32, multiplier: i32, divisor: i32) -> i32 {
    let numerator = i64::from(value) * i64::from(multiplier);
    let divisor = i64::from(divisor);
    ((numerator + numerator.signum() * (divisor / 2)) / divisor) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::{
        graph_window::{Animation, Keyframe, Node},
        session_input_abi::SESSION_INPUT_SOURCE_MOUSE,
    };

    fn scene() -> Scene {
        Scene::new(
            140,
            90,
            Rect {
                x: 0,
                y: 0,
                width: 140,
                height: 12,
            },
            &[Node::rectangle(
                Rect {
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 4,
                },
                0,
            )],
            Animation {
                node_index: 0,
                period_frames: 8,
                keyframes: [
                    Keyframe { frame: 0, x: 0 },
                    Keyframe { frame: 2, x: 1 },
                    Keyframe { frame: 4, x: 2 },
                    Keyframe { frame: 6, x: 1 },
                ],
            },
        )
        .unwrap()
    }

    fn event(sequence: u64, kind: u16, value0: i32, flags: u32) -> SessionInputEventV1 {
        SessionInputEventV1 {
            sequence,
            kind,
            value0,
            flags,
            source: SESSION_INPUT_SOURCE_MOUSE,
            ..SessionInputEventV1::empty()
        }
    }

    #[test]
    fn a_gap_cancels_a_lost_release_and_admits_the_next_press() {
        let mut layout = Layout::new([1024, 768], &scene(), None);
        let origin = layout.position;
        let button = SESSION_INPUT_KIND_MOUSE_BUTTON_STATE;
        let motion = SESSION_INPUT_KIND_RELATIVE_MOTION;
        assert!(matches!(
            layout.input(event(0, button, 1, 0)),
            Update::Captured(0)
        ));
        layout.input(event(1, motion, 10, 0));
        assert_eq!(layout.position, [origin[0] + 10, origin[1]]);
        assert!(matches!(
            layout.input(event(3, button, 1, SESSION_INPUT_FLAG_GAP_BEFORE)),
            Update::Captured(3)
        ));
        assert_eq!(layout.position, origin);
        layout.input(event(4, motion, 5, 0));
        let Update::Dropped(drop) = layout.input(event(5, button, 0, 0)) else {
            panic!("the fresh gesture should commit");
        };
        assert_eq!(drop.sequence, 5);
        assert_eq!(layout.position_for(drop.pose), [origin[0] + 5, origin[1]]);
    }

    #[test]
    fn placement_round_trips_to_the_same_pixel() {
        let scene = scene();
        for view in [[600, 400], [1024, 768], [1280, 1024]] {
            let mut layout = Layout::new(view, &scene, None);
            for position in [[0, 0], [1, 1], layout.position, layout.bounds] {
                layout.position = position;
                let pose = layout.pose();
                assert!(pose.valid());
                assert_eq!(Layout::new(view, &scene, Some(pose)).position, position);
            }
        }
    }
}
