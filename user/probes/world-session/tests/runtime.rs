use pythos_shared::{
    session_input_abi::{
        KEY_B, KEY_C, KEY_D, KEY_DIGIT1, KEY_DIGIT3, KEY_DIGIT4, KEY_F, KEY_G, KEY_H, KEY_T, KEY_W,
        SESSION_INPUT_KIND_KEY_DOWN, SESSION_INPUT_KIND_MOUSE_BUTTON_STATE,
        SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_SOURCE_MOUSE, SessionInputEventV1,
    },
    world_session::{Frame, HALF_WIDTH_MM, Invocation, Kind, Notice, Object, World},
};
use pythos_user_world_session::step;

fn key(frame: Frame, sequence: u64, tag: u16) -> Frame {
    step(Invocation {
        frame,
        event: SessionInputEventV1 {
            sequence,
            kind: SESSION_INPUT_KIND_KEY_DOWN,
            source: SESSION_INPUT_SOURCE_KEYBOARD,
            value0: i32::from(tag),
            ..SessionInputEventV1::empty()
        },
    })
}

#[test]
fn blank_half_acre_keeps_walking_transient_and_bounded() {
    let initial = Frame::restored(World::blank());
    assert_eq!(initial.world.objects().count(), 0);
    assert_eq!(
        initial.world,
        World::decode(&initial.world.encode()).unwrap()
    );
    let mut frame = initial;
    for sequence in 1..=25 {
        frame = key(frame, sequence, KEY_W);
    }
    assert_eq!(frame.world, initial.world);
    assert_eq!(frame.view.pose.z, -HALF_WIDTH_MM);
}

#[test]
fn same_prop_runs_latest_binding_from_a_saved_world() {
    let mut frame = Frame::restored(World::blank());
    for (i, tag) in [
        KEY_W, KEY_DIGIT1, KEY_D, KEY_DIGIT3, KEY_D, KEY_DIGIT3, KEY_C, KEY_T, KEY_B, KEY_F, KEY_T,
        KEY_B,
    ]
    .into_iter()
    .enumerate()
    {
        frame = key(frame, i as u64 + 1, tag);
    }
    assert_eq!(frame.world.revision, 6);
    assert_eq!(frame.world.focus, 2);
    assert_eq!(frame.world.object(1).unwrap().target, 3);
    assert_eq!(frame.world.object(1).unwrap().pose.z, -4000);
    let saved = World::decode(&frame.world.encode()).unwrap();
    let frame = key(Frame::restored(saved), 1, KEY_C);
    let frame = key(frame, 2, KEY_F);
    assert_eq!(frame.world.focus, 3);
    assert_eq!(frame.world.revision, 7);
    assert_eq!(frame.world.receipt.actor, 1);
    assert_eq!(frame.world.receipt.target, 3);
}

#[test]
fn template_instances_preserve_local_targets_and_pose_round_trips() {
    let original = key(Frame::restored(World::blank()), 1, KEY_DIGIT4);
    let moved = key(original, 2, KEY_D);
    let frame = key(moved, 3, KEY_DIGIT4);
    assert_eq!(frame.world.object(1).unwrap().target, 2);
    assert_eq!(frame.world.object(3).unwrap().target, 4);
    assert_eq!(frame.world.object(2).unwrap().kind, Kind::Screen);
    assert_eq!(Frame::decode(&frame.encode()).unwrap(), frame);
    assert_eq!(frame.world.revision, 2);
}

#[test]
fn a_standing_anchor_is_a_distinct_destination_from_arrival() {
    let mut frame = key(Frame::restored(World::blank()), 1, KEY_DIGIT3);
    frame = key(frame, 2, KEY_G);
    assert_eq!(frame.view.pose, frame.world.object(1).unwrap().pose);
    assert_ne!(frame.view.pose, frame.world.arrival);
    frame = key(frame, 3, KEY_D);
    frame = key(frame, 4, KEY_H);
    let restored = Frame::restored(World::decode(&frame.world.encode()).unwrap());
    assert_eq!(restored.view.pose, frame.world.arrival);
    assert_ne!(
        restored.world.arrival,
        restored.world.object(1).unwrap().pose
    );
}

#[test]
fn placement_outside_ground_leaves_world_revision_unchanged() {
    let mut frame = Frame::restored(World::blank());
    for sequence in 1..=22 {
        frame = key(frame, sequence, KEY_W);
    }
    frame = key(frame, 23, KEY_DIGIT1);
    assert_eq!(frame.world, World::blank());
    assert_eq!(frame.view.notice, Notice::Outside);
}

#[test]
fn native_input_can_start_at_sequence_zero_and_commit() {
    let event = SessionInputEventV1 {
        sequence: 0,
        kind: SESSION_INPUT_KIND_KEY_DOWN,
        source: SESSION_INPUT_SOURCE_KEYBOARD,
        value0: i32::from(KEY_DIGIT1),
        ..SessionInputEventV1::empty()
    };
    let input = Invocation {
        frame: Frame::restored(World::blank()),
        event,
    };
    let decoded = Invocation::decode(&input.encode()).unwrap();
    let output = Frame::decode(&step(decoded).encode()).unwrap();
    assert_eq!(
        output
            .validate_result(input)
            .unwrap()
            .world
            .receipt
            .sequence,
        0
    );
}

#[test]
fn a_walk_event_cannot_commit_a_placement() {
    let frame = Frame::restored(World::blank());
    let output = key(frame, 1, KEY_DIGIT1);
    let input = Invocation {
        frame,
        event: SessionInputEventV1 {
            sequence: 1,
            kind: SESSION_INPUT_KIND_KEY_DOWN,
            source: SESSION_INPUT_SOURCE_KEYBOARD,
            value0: i32::from(KEY_W),
            ..SessionInputEventV1::empty()
        },
    };
    assert!(output.validate_result(input).is_err());
}

#[test]
fn a_bind_receipt_cannot_remove_unrelated_objects() {
    let frame = key(Frame::restored(World::blank()), 1, KEY_DIGIT4);
    let input = Invocation {
        frame,
        event: SessionInputEventV1 {
            sequence: 2,
            kind: SESSION_INPUT_KIND_KEY_DOWN,
            source: SESSION_INPUT_SOURCE_KEYBOARD,
            value0: i32::from(KEY_B),
            ..SessionInputEventV1::empty()
        },
    };
    let mut forged = step(input);
    forged.world.slots.fill(Object::EMPTY);
    forged.view.selected = 0;
    forged.view.target = 0;
    let forged = Frame::decode(&forged.encode()).unwrap();
    assert!(forged.validate_result(input).is_err());
}

#[test]
fn ignored_pointer_packets_keep_the_edit_feedback_visible() {
    let frame = key(Frame::restored(World::blank()), 1, KEY_DIGIT4);
    let next = step(Invocation {
        frame,
        event: SessionInputEventV1 {
            sequence: 2,
            kind: SESSION_INPUT_KIND_MOUSE_BUTTON_STATE,
            source: SESSION_INPUT_SOURCE_MOUSE,
            ..SessionInputEventV1::empty()
        },
    });
    assert_eq!(next, frame);
    assert_eq!(next.view.notice, Notice::Saved);
}
