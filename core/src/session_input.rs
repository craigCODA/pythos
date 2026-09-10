//! Device-neutral session-input delivery queue.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};

use crate::{
    input_drivers::{KeyCode, RawInputEvent},
    input_events::{self, InputEventKind, InputSource},
    service_identity::ServiceId,
};
use pythos_shared::session_input_abi::{
    KEY_A, KEY_B, KEY_BACKSPACE, KEY_C, KEY_D, KEY_DIGIT0, KEY_DIGIT1, KEY_DIGIT2, KEY_DIGIT3,
    KEY_DIGIT4, KEY_DIGIT5, KEY_DIGIT6, KEY_DIGIT7, KEY_DIGIT8, KEY_DIGIT9, KEY_E, KEY_ENTER,
    KEY_ESCAPE, KEY_F, KEY_G, KEY_H, KEY_I, KEY_J, KEY_K, KEY_L, KEY_M, KEY_N, KEY_O, KEY_P, KEY_Q,
    KEY_R, KEY_S, KEY_SPACE, KEY_T, KEY_U, KEY_V, KEY_W, KEY_X, KEY_Y, KEY_Z,
    SESSION_INPUT_FLAG_GAP_BEFORE, SESSION_INPUT_KIND_KEY_DOWN,
    SESSION_INPUT_KIND_MOUSE_BUTTON_STATE, SESSION_INPUT_KIND_RELATIVE_MOTION,
    SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_SOURCE_MOUSE, SessionInputEventV1,
};

const QUEUE_CAPACITY: usize = 16;
const MODE_COMPATIBILITY: u8 = 0;
const MODE_SESSION: u8 = 1;
const MODE_BINDING: u8 = 2;

#[derive(Clone, Copy)]
struct SequencedRawInputEvent {
    sequence: u64,
    raw: RawInputEvent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConsumerMode {
    Compatibility,
    Session { holder: ServiceId, expected: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishOutcome {
    Enqueued,
    DroppedNewest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionInputError {
    AlreadyBound,
    SessionBound,
    SessionUnbound,
    WrongHolder,
}

/// Fixed-size SPSC input queue. The producer is one IRQ top half at a time;
/// the consumer is either the legacy compatibility path or the one bound
/// session. Binding is a pre-initialization, quiescent operation: it must run
/// before `ps2::initialize()` permits producers to publish, and there is no
/// live producer/consumer transition or reset path in this slice.
pub(crate) struct SessionInputQueue {
    slots: UnsafeCell<[Option<SequencedRawInputEvent>; QUEUE_CAPACITY]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    next_sequence: AtomicU64,
    mode: AtomicU8,
    holder: AtomicU64,
    expected: AtomicU64,
}

// SAFETY:
// 1. Invariant: one serialized IRQ producer writes only `tail`, and one
//    normal-context consumer writes only `head`.
// 2. Established by: the current single-core PIC delivery and the exclusive
//    consumer-mode contract below.
// 3. Lifetime: the production queue is static; test queues live for each test.
// 4. Pointer ownership: only producer/consumer touch their owned slot index.
// 5. Alignment: `UnsafeCell` preserves the array alignment.
// 6. Mapped length: all indices are reduced modulo `QUEUE_CAPACITY`.
// 7. Concurrency: Release/Acquire index publication prevents observing a
//    partially-written slot.
// 8. Violation: multiple producers or consumers could race a slot access.
unsafe impl Sync for SessionInputQueue {}

impl SessionInputQueue {
    pub(crate) const fn new() -> Self {
        Self {
            slots: UnsafeCell::new([None; QUEUE_CAPACITY]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            next_sequence: AtomicU64::new(0),
            mode: AtomicU8::new(MODE_COMPATIBILITY),
            holder: AtomicU64::new(0),
            expected: AtomicU64::new(0),
        }
    }

    pub(crate) fn publish(&self, raw: RawInputEvent) -> PublishOutcome {
        // A candidate has a sequence even when it cannot occupy a physical
        // slot, so a session consumer can observe loss without producer-side
        // policy or blocking.
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) % QUEUE_CAPACITY;
        if next_tail == self.head.load(Ordering::Acquire) {
            return PublishOutcome::DroppedNewest;
        }

        // SAFETY: `tail` is owned exclusively by the serialized IRQ producer;
        // the consumer cannot read it until the Release store of `tail` below.
        unsafe {
            (*self.slots.get())[tail] = Some(SequencedRawInputEvent { sequence, raw });
        }
        self.tail.store(next_tail, Ordering::Release);
        PublishOutcome::Enqueued
    }

    pub(crate) fn bind_session_consumer_quiescent(&self, holder: ServiceId) -> Result<(), SessionInputError> {
        if self
            .mode
            .compare_exchange(
                MODE_COMPATIBILITY,
                MODE_BINDING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return Err(SessionInputError::AlreadyBound);
        }

        // This is intentionally a flush rather than delivery of events that
        // preceded the session's authority. The quiescent caller has not yet
        // enabled PS/2 publication, so tail cannot change during the sample.
        let tail = self.tail.load(Ordering::Acquire);
        self.head.store(tail, Ordering::Release);
        self.expected.store(
            self.next_sequence.load(Ordering::Acquire),
            Ordering::Relaxed,
        );
        self.holder.store(holder.raw(), Ordering::Relaxed);
        self.mode.store(MODE_SESSION, Ordering::Release);
        Ok(())
    }

    fn try_read_compatibility(&self) -> Result<Option<RawInputEvent>, SessionInputError> {
        if self.mode.load(Ordering::Acquire) != MODE_COMPATIBILITY {
            return Err(SessionInputError::SessionBound);
        }
        match self.consumer_mode() {
            ConsumerMode::Compatibility => Ok(self.pop().map(|event| event.raw)),
            ConsumerMode::Session { .. } => Err(SessionInputError::SessionBound),
        }
    }

    pub(crate) fn try_read_session(
        &self,
        holder: ServiceId,
    ) -> Result<Option<SessionInputEventV1>, SessionInputError> {
        let ConsumerMode::Session {
            holder: bound_holder,
            expected,
        } = self.consumer_mode()
        else {
            return Err(SessionInputError::SessionUnbound);
        };
        if holder != bound_holder {
            return Err(SessionInputError::WrongHolder);
        }
        let Some(event) = self.pop() else {
            return Ok(None);
        };
        let flags = if event.sequence == expected {
            0
        } else {
            SESSION_INPUT_FLAG_GAP_BEFORE
        };
        self.expected
            .store(event.sequence.wrapping_add(1), Ordering::Relaxed);
        Ok(Some(to_session_event(event.sequence, event.raw, flags)))
    }

    fn consumer_mode(&self) -> ConsumerMode {
        match self.mode.load(Ordering::Acquire) {
            MODE_COMPATIBILITY | MODE_BINDING => ConsumerMode::Compatibility,
            MODE_SESSION => ConsumerMode::Session {
                holder: ServiceId::from_raw(self.holder.load(Ordering::Relaxed)),
                expected: self.expected.load(Ordering::Relaxed),
            },
            _ => ConsumerMode::Compatibility,
        }
    }

    pub(crate) fn session_ready(&self, _holder: ServiceId) -> Result<bool, SessionInputError> {
        Ok(false)
    }

    fn pop(&self) -> Option<SequencedRawInputEvent> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        // SAFETY: `head` is owned exclusively by the selected consumer, and
        // the producer published the initialized slot with a Release tail.
        let event = unsafe { (*self.slots.get())[head].take() };
        self.head
            .store((head + 1) % QUEUE_CAPACITY, Ordering::Release);
        event
    }
}

static SESSION_INPUT_QUEUE: SessionInputQueue = SessionInputQueue::new();

/// Publish a normalized-raw device event from IRQ context. This only writes a
/// bounded queue and never normalizes, looks up capability, renders, blocks,
/// or interprets session controls.
pub fn publish(raw: RawInputEvent) -> PublishOutcome {
    SESSION_INPUT_QUEUE.publish(raw)
}

/// Bind the sole session consumer before producers are initialized.
pub fn bind_session_consumer_quiescent(holder: ServiceId) -> Result<(), SessionInputError> {
    SESSION_INPUT_QUEUE.bind_session_consumer_quiescent(holder)
}

/// Read one sequence-stamped ABI event as the bound session holder.
pub fn try_read_session(
    holder: ServiceId,
) -> Result<Option<SessionInputEventV1>, SessionInputError> {
    SESSION_INPUT_QUEUE.try_read_session(holder)
}

/// Temporary legacy consumer for the launcher path, available only before a
/// session holder binds.
pub fn try_read_compatibility() -> Result<Option<RawInputEvent>, SessionInputError> {
    SESSION_INPUT_QUEUE.try_read_compatibility()
}

fn to_session_event(sequence: u64, raw: RawInputEvent, flags: u32) -> SessionInputEventV1 {
    let event = input_events::normalize(raw).expect("RawInputEvent is exhaustively normalizable");
    let (kind, source, value0, value1) = match event.kind {
        InputEventKind::KeyDown(key) => (
            SESSION_INPUT_KIND_KEY_DOWN,
            SESSION_INPUT_SOURCE_KEYBOARD,
            i32::from(key_tag(key)),
            0,
        ),
        InputEventKind::RelativeMotion(motion) => (
            SESSION_INPUT_KIND_RELATIVE_MOTION,
            SESSION_INPUT_SOURCE_MOUSE,
            i32::from(motion.dx),
            i32::from(motion.dy),
        ),
        InputEventKind::PointerButton { left } => (
            SESSION_INPUT_KIND_MOUSE_BUTTON_STATE,
            SESSION_INPUT_SOURCE_MOUSE,
            if left { 1 } else { 0 },
            0,
        ),
    };
    debug_assert!(matches!(
        event.source,
        InputSource::Keyboard | InputSource::Mouse
    ));
    SessionInputEventV1 {
        sequence,
        kind,
        source,
        flags,
        value0,
        value1,
        reserved0: 0,
        reserved1: 0,
    }
}

fn key_tag(key: KeyCode) -> u16 {
    match key {
        KeyCode::A => KEY_A,
        KeyCode::B => KEY_B,
        KeyCode::C => KEY_C,
        KeyCode::D => KEY_D,
        KeyCode::E => KEY_E,
        KeyCode::F => KEY_F,
        KeyCode::G => KEY_G,
        KeyCode::H => KEY_H,
        KeyCode::I => KEY_I,
        KeyCode::J => KEY_J,
        KeyCode::K => KEY_K,
        KeyCode::L => KEY_L,
        KeyCode::M => KEY_M,
        KeyCode::N => KEY_N,
        KeyCode::O => KEY_O,
        KeyCode::P => KEY_P,
        KeyCode::Q => KEY_Q,
        KeyCode::R => KEY_R,
        KeyCode::S => KEY_S,
        KeyCode::T => KEY_T,
        KeyCode::U => KEY_U,
        KeyCode::V => KEY_V,
        KeyCode::W => KEY_W,
        KeyCode::X => KEY_X,
        KeyCode::Y => KEY_Y,
        KeyCode::Z => KEY_Z,
        KeyCode::Digit0 => KEY_DIGIT0,
        KeyCode::Digit1 => KEY_DIGIT1,
        KeyCode::Digit2 => KEY_DIGIT2,
        KeyCode::Digit3 => KEY_DIGIT3,
        KeyCode::Digit4 => KEY_DIGIT4,
        KeyCode::Digit5 => KEY_DIGIT5,
        KeyCode::Digit6 => KEY_DIGIT6,
        KeyCode::Digit7 => KEY_DIGIT7,
        KeyCode::Digit8 => KEY_DIGIT8,
        KeyCode::Digit9 => KEY_DIGIT9,
        KeyCode::Enter => KEY_ENTER,
        KeyCode::Escape => KEY_ESCAPE,
        KeyCode::Space => KEY_SPACE,
        KeyCode::Backspace => KEY_BACKSPACE,
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::Ordering;

    use super::*;
    use crate::input_drivers::{KeyCode, RawInputEvent};
    use crate::input_events::{InputEventKind, InputSource, RelativeMotion};
    use crate::service_identity::ServiceId;
    use pythos_shared::session_input_abi::{
        KEY_A, KEY_B, KEY_BACKSPACE, KEY_C, KEY_D, KEY_DIGIT0, KEY_DIGIT1, KEY_DIGIT2, KEY_DIGIT3,
        KEY_DIGIT4, KEY_DIGIT5, KEY_DIGIT6, KEY_DIGIT7, KEY_DIGIT8, KEY_DIGIT9, KEY_E, KEY_ENTER,
        KEY_ESCAPE, KEY_F, KEY_G, KEY_H, KEY_I, KEY_J, KEY_K, KEY_L, KEY_M, KEY_N, KEY_O, KEY_P,
        KEY_Q, KEY_R, KEY_S, KEY_SPACE, KEY_T, KEY_U, KEY_V, KEY_W, KEY_X, KEY_Y, KEY_Z,
        SESSION_INPUT_FLAG_GAP_BEFORE, SESSION_INPUT_KIND_KEY_DOWN,
        SESSION_INPUT_KIND_MOUSE_BUTTON_STATE, SESSION_INPUT_KIND_RELATIVE_MOTION,
        SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_SOURCE_MOUSE, SessionInputEventV1,
    };

    fn mouse(ordinal: i8) -> RawInputEvent {
        RawInputEvent::MouseMoved {
            dx: ordinal,
            dy: -ordinal,
        }
    }

    #[test]
    fn readiness_checks_owner_without_consuming_or_advancing_sequence() {
        let queue = SessionInputQueue::new();
        let holder = ServiceId::from_raw(7);
        assert_eq!(queue.session_ready(holder), Err(SessionInputError::SessionUnbound));
        queue.bind_session_consumer_quiescent(holder).unwrap();
        assert_eq!(queue.session_ready(holder), Ok(false));
        queue.publish(mouse(3));
        for _ in 0..2 {
            assert_eq!(queue.session_ready(ServiceId::from_raw(8)), Err(SessionInputError::WrongHolder));
            assert_eq!(queue.session_ready(holder), Ok(true));
            assert_eq!(queue.expected.load(Ordering::Relaxed), 0);
            assert_eq!(queue.head.load(Ordering::Relaxed), 0);
            assert_eq!(queue.next_sequence.load(Ordering::Relaxed), 1);
        }
        assert_eq!(queue.try_read_session(holder).unwrap().unwrap().sequence, 0);
        assert_eq!(queue.session_ready(holder), Ok(false));
        assert_eq!(queue.try_read_compatibility(), Err(SessionInputError::SessionBound));
        assert_eq!(queue.bind_session_consumer_quiescent(holder), Err(SessionInputError::AlreadyBound));
    }

    fn key(key: KeyCode) -> RawInputEvent {
        RawInputEvent::KeyPressed { scancode: 0, key }
    }

    #[test]
    fn fifteen_unread_entries_are_preserved_and_newest_is_dropped() {
        let queue = SessionInputQueue::new();
        for ordinal in 0..15 {
            assert_eq!(queue.publish(mouse(ordinal)), PublishOutcome::Enqueued);
        }
        assert_eq!(queue.publish(mouse(99)), PublishOutcome::DroppedNewest);
        for ordinal in 0..15 {
            assert_eq!(
                queue.try_read_compatibility().unwrap(),
                Some(mouse(ordinal))
            );
        }
    }

    #[test]
    fn dropped_candidate_consumes_sequence_and_sets_gap_before() {
        let queue = SessionInputQueue::new();
        let holder = ServiceId::from_raw(7);
        queue.bind_session_consumer_quiescent(holder).unwrap();
        for ordinal in 0..15 {
            queue.publish(mouse(ordinal));
        }
        queue.publish(mouse(99));
        for _ in 0..15 {
            queue.try_read_session(holder).unwrap().unwrap();
        }
        queue.publish(mouse(100));
        let after_gap = queue.try_read_session(holder).unwrap().unwrap();
        assert_eq!(after_gap.flags, SESSION_INPUT_FLAG_GAP_BEFORE);
        assert_eq!(after_gap.sequence, 16);
    }

    #[test]
    fn bind_flushes_stale_events_and_denies_compatibility_consumer() {
        let queue = SessionInputQueue::new();
        queue.publish(key(KeyCode::A));
        queue
            .bind_session_consumer_quiescent(ServiceId::from_raw(9))
            .unwrap();
        assert_eq!(queue.try_read_session(ServiceId::from_raw(9)), Ok(None));
        assert_eq!(
            queue.try_read_compatibility(),
            Err(SessionInputError::SessionBound)
        );
    }

    #[test]
    fn session_reads_reject_a_holder_other_than_the_bound_consumer() {
        let queue = SessionInputQueue::new();
        queue
            .bind_session_consumer_quiescent(ServiceId::from_raw(9))
            .unwrap();

        assert_eq!(
            queue.try_read_session(ServiceId::from_raw(10)),
            Err(SessionInputError::WrongHolder)
        );
    }

    #[test]
    fn second_session_binding_is_denied() {
        let queue = SessionInputQueue::new();
        queue
            .bind_session_consumer_quiescent(ServiceId::from_raw(9))
            .unwrap();

        assert_eq!(
            queue.bind_session_consumer_quiescent(ServiceId::from_raw(10)),
            Err(SessionInputError::AlreadyBound)
        );
    }

    #[test]
    fn sequence_continues_across_u64_wrap_without_a_gap() {
        let queue = SessionInputQueue::new();
        queue.next_sequence.store(u64::MAX, Ordering::Relaxed);
        let holder = ServiceId::from_raw(12);
        queue.bind_session_consumer_quiescent(holder).unwrap();
        queue.publish(mouse(1));
        queue.publish(mouse(2));

        let first = queue.try_read_session(holder).unwrap().unwrap();
        let second = queue.try_read_session(holder).unwrap().unwrap();
        assert_eq!(first.sequence, u64::MAX);
        assert_eq!(first.flags, 0);
        assert_eq!(second.sequence, 0);
        assert_eq!(second.flags, 0);
    }

    #[test]
    fn every_normalized_event_maps_to_the_shared_wire_abi_with_zero_reserved_fields() {
        let keys = [
            (KeyCode::A, KEY_A),
            (KeyCode::B, KEY_B),
            (KeyCode::C, KEY_C),
            (KeyCode::D, KEY_D),
            (KeyCode::E, KEY_E),
            (KeyCode::F, KEY_F),
            (KeyCode::G, KEY_G),
            (KeyCode::H, KEY_H),
            (KeyCode::I, KEY_I),
            (KeyCode::J, KEY_J),
            (KeyCode::K, KEY_K),
            (KeyCode::L, KEY_L),
            (KeyCode::M, KEY_M),
            (KeyCode::N, KEY_N),
            (KeyCode::O, KEY_O),
            (KeyCode::P, KEY_P),
            (KeyCode::Q, KEY_Q),
            (KeyCode::R, KEY_R),
            (KeyCode::S, KEY_S),
            (KeyCode::T, KEY_T),
            (KeyCode::U, KEY_U),
            (KeyCode::V, KEY_V),
            (KeyCode::W, KEY_W),
            (KeyCode::X, KEY_X),
            (KeyCode::Y, KEY_Y),
            (KeyCode::Z, KEY_Z),
            (KeyCode::Digit0, KEY_DIGIT0),
            (KeyCode::Digit1, KEY_DIGIT1),
            (KeyCode::Digit2, KEY_DIGIT2),
            (KeyCode::Digit3, KEY_DIGIT3),
            (KeyCode::Digit4, KEY_DIGIT4),
            (KeyCode::Digit5, KEY_DIGIT5),
            (KeyCode::Digit6, KEY_DIGIT6),
            (KeyCode::Digit7, KEY_DIGIT7),
            (KeyCode::Digit8, KEY_DIGIT8),
            (KeyCode::Digit9, KEY_DIGIT9),
            (KeyCode::Enter, KEY_ENTER),
            (KeyCode::Escape, KEY_ESCAPE),
            (KeyCode::Space, KEY_SPACE),
            (KeyCode::Backspace, KEY_BACKSPACE),
        ];
        for (key_code, tag) in keys {
            assert_eq!(
                to_session_event(41, key(key_code), 0),
                SessionInputEventV1 {
                    sequence: 41,
                    kind: SESSION_INPUT_KIND_KEY_DOWN,
                    source: SESSION_INPUT_SOURCE_KEYBOARD,
                    flags: 0,
                    value0: i32::from(tag),
                    value1: 0,
                    reserved0: 0,
                    reserved1: 0,
                }
            );
        }
        assert_eq!(
            to_session_event(42, RawInputEvent::MouseMoved { dx: -4, dy: 7 }, 0),
            SessionInputEventV1 {
                sequence: 42,
                kind: SESSION_INPUT_KIND_RELATIVE_MOTION,
                source: SESSION_INPUT_SOURCE_MOUSE,
                flags: 0,
                value0: -4,
                value1: 7,
                reserved0: 0,
                reserved1: 0,
            }
        );
        assert_eq!(
            to_session_event(43, RawInputEvent::MouseButton { left: true }, 0),
            SessionInputEventV1 {
                sequence: 43,
                kind: SESSION_INPUT_KIND_MOUSE_BUTTON_STATE,
                source: SESSION_INPUT_SOURCE_MOUSE,
                flags: 0,
                value0: 1,
                value1: 0,
                reserved0: 0,
                reserved1: 0,
            }
        );
        assert_eq!(
            to_session_event(44, RawInputEvent::MouseButton { left: false }, 0),
            SessionInputEventV1 {
                sequence: 44,
                kind: SESSION_INPUT_KIND_MOUSE_BUTTON_STATE,
                source: SESSION_INPUT_SOURCE_MOUSE,
                flags: 0,
                value0: 0,
                value1: 0,
                reserved0: 0,
                reserved1: 0,
            }
        );

        assert_eq!(
            crate::input_events::normalize(key(KeyCode::A))
                .unwrap()
                .kind,
            InputEventKind::KeyDown(KeyCode::A)
        );
        assert_eq!(
            crate::input_events::normalize(RawInputEvent::MouseMoved { dx: -4, dy: 7 }).unwrap(),
            crate::input_events::InputEvent {
                source: InputSource::Mouse,
                kind: InputEventKind::RelativeMotion(RelativeMotion { dx: -4, dy: 7 }),
            }
        );
    }
}
