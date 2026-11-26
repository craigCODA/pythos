#![no_std]

use pythos_shared::{
    session_input_abi::*,
    world_session::{Edit, Frame, HALF_WIDTH_MM, Invocation, Kind, Notice, Pose, World},
};

pub fn step(input: Invocation) -> Frame {
    let mut frame = input.frame;
    if input.event.flags & SESSION_INPUT_FLAG_GAP_BEFORE != 0 {
        clear_selection(&mut frame);
        frame.view.notice = Notice::InputGap;
        return frame;
    }
    if let Some(result) = frame.world.edited_for(frame.view, input.event) {
        apply_result(&mut frame, result);
        return frame;
    }
    match input.event.kind {
        SESSION_INPUT_KIND_KEY_DOWN => handle_key(&mut frame, input.event.value0),
        SESSION_INPUT_KIND_RELATIVE_MOTION if input.event.value0.unsigned_abs() >= 8 => {
            turn(&mut frame.view.pose, input.event.value0.signum() as i8);
            frame.view.notice = Notice::Ready;
        }
        _ => {}
    }
    frame
}

fn apply_result(frame: &mut Frame, result: Result<World, Notice>) {
    match result {
        Ok(world) => {
            frame.world = world;
            frame.view.notice = Notice::Saved;
            match world.receipt.edit {
                Edit::Place | Edit::Template => {
                    frame.view.selected = world.receipt.actor;
                    frame.view.target = world.receipt.target;
                }
                Edit::Remove => clear_selection(frame),
                Edit::Use => frame.view.notice = Notice::Focused,
                _ => {}
            }
        }
        Err(notice) => frame.view.notice = notice,
    }
}

fn handle_key(frame: &mut Frame, raw: i32) {
    let Ok(key) = u16::try_from(raw) else { return };
    match key {
        KEY_W => walk(&mut frame.view.pose, 1000, 0),
        KEY_S => walk(&mut frame.view.pose, -1000, 0),
        KEY_A => walk(&mut frame.view.pose, 0, -1000),
        KEY_D => walk(&mut frame.view.pose, 0, 1000),
        KEY_Q => turn(&mut frame.view.pose, -1),
        KEY_E => turn(&mut frame.view.pose, 1),
        KEY_C => {
            frame.view.selected = next_object(&frame.world, frame.view.selected, 0);
            frame.view.target = frame
                .world
                .object(frame.view.selected)
                .map_or(0, |object| object.target);
        }
        KEY_T => {
            frame.view.target = next_object(&frame.world, frame.view.target, frame.view.selected)
        }
        KEY_ESCAPE => clear_selection(frame),
        KEY_G => {
            frame.view.notice = Notice::Select;
            if let Some(anchor) = frame.world.object(frame.view.selected)
                && anchor.kind == Kind::Anchor
            {
                frame.view.pose = anchor.pose;
                frame.view.notice = Notice::ToAnchor;
            }
            return;
        }
        KEY_DIGIT0 => {
            frame.view.notice = Notice::Exit;
            return;
        }
        _ => return,
    }
    frame.view.notice = Notice::Ready;
}

fn clear_selection(frame: &mut Frame) {
    frame.view.selected = 0;
    frame.view.target = 0;
}

fn walk(pose: &mut Pose, forward: i32, right: i32) {
    let next = pose.offset(forward, right);
    pose.x = next.x.clamp(-HALF_WIDTH_MM, HALF_WIDTH_MM);
    pose.z = next.z.clamp(-HALF_WIDTH_MM, HALF_WIDTH_MM);
}

fn turn(pose: &mut Pose, amount: i8) {
    pose.heading = (pose.heading as i8 + amount).rem_euclid(16) as u8;
}

fn next_object(world: &World, current: u32, excluded: u32) -> u32 {
    let ids = world
        .objects()
        .map(|object| object.id)
        .filter(|id| *id != excluded);
    ids.clone()
        .filter(|id| *id > current)
        .min()
        .or_else(|| ids.min())
        .unwrap_or(0)
}
