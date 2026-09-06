//! Versioned, device-neutral session-input wire ABI.

pub const SESSION_INPUT_ABI_MAJOR: u16 = 1;
pub const SESSION_INPUT_ABI_MINOR: u16 = 0;

pub const SYSCALL_SESSION_INPUT_TRY_READ: u64 = 0x5059_0150;
pub const SESSION_INPUT_RESULT_EVENT: u64 = 0x5059_004F;
pub const SESSION_INPUT_RESULT_EMPTY: u64 = 0x5059_0150_0000;
pub const SESSION_INPUT_RESOURCE_ID: u64 = 0x1A50_0100;

pub const SESSION_INPUT_SOURCE_KEYBOARD: u16 = 1;
pub const SESSION_INPUT_SOURCE_MOUSE: u16 = 2;

pub const SESSION_INPUT_KIND_KEY_DOWN: u16 = 1;
pub const SESSION_INPUT_KIND_RELATIVE_MOTION: u16 = 2;
pub const SESSION_INPUT_KIND_MOUSE_BUTTON_STATE: u16 = 3;

pub const SESSION_INPUT_FLAG_GAP_BEFORE: u32 = 0x0000_0001;

// Logical key values are PythOS ABI tags, not Rust enum discriminants, PS/2
// scan codes, or USB usages.
pub const KEY_A: u16 = 0x0001;
pub const KEY_B: u16 = 0x0002;
pub const KEY_C: u16 = 0x0003;
pub const KEY_D: u16 = 0x0004;
pub const KEY_E: u16 = 0x0005;
pub const KEY_F: u16 = 0x0006;
pub const KEY_G: u16 = 0x0007;
pub const KEY_H: u16 = 0x0008;
pub const KEY_I: u16 = 0x0009;
pub const KEY_J: u16 = 0x000A;
pub const KEY_K: u16 = 0x000B;
pub const KEY_L: u16 = 0x000C;
pub const KEY_M: u16 = 0x000D;
pub const KEY_N: u16 = 0x000E;
pub const KEY_O: u16 = 0x000F;
pub const KEY_P: u16 = 0x0010;
pub const KEY_Q: u16 = 0x0011;
pub const KEY_R: u16 = 0x0012;
pub const KEY_S: u16 = 0x0013;
pub const KEY_T: u16 = 0x0014;
pub const KEY_U: u16 = 0x0015;
pub const KEY_V: u16 = 0x0016;
pub const KEY_W: u16 = 0x0017;
pub const KEY_X: u16 = 0x0018;
pub const KEY_Y: u16 = 0x0019;
pub const KEY_Z: u16 = 0x001A;

pub const KEY_DIGIT0: u16 = 0x0020;
pub const KEY_DIGIT1: u16 = 0x0021;
pub const KEY_DIGIT2: u16 = 0x0022;
pub const KEY_DIGIT3: u16 = 0x0023;
pub const KEY_DIGIT4: u16 = 0x0024;
pub const KEY_DIGIT5: u16 = 0x0025;
pub const KEY_DIGIT6: u16 = 0x0026;
pub const KEY_DIGIT7: u16 = 0x0027;
pub const KEY_DIGIT8: u16 = 0x0028;
pub const KEY_DIGIT9: u16 = 0x0029;

pub const KEY_ENTER: u16 = 0x0030;
pub const KEY_ESCAPE: u16 = 0x0031;
pub const KEY_SPACE: u16 = 0x0032;
pub const KEY_BACKSPACE: u16 = 0x0033;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionInputEventV1 {
    pub sequence: u64,
    pub kind: u16,
    pub source: u16,
    pub flags: u32,
    pub value0: i32,
    pub value1: i32,
    pub reserved0: u64,
    pub reserved1: u64,
}

impl SessionInputEventV1 {
    pub const fn empty() -> Self {
        Self {
            sequence: 0,
            kind: 0,
            source: 0,
            flags: 0,
            value0: 0,
            value1: 0,
            reserved0: 0,
            reserved1: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_v1_layout_is_exact_and_reserved_fields_start_zero() {
        assert_eq!(SESSION_INPUT_ABI_MAJOR, 1);
        assert_eq!(SESSION_INPUT_ABI_MINOR, 0);
        assert_eq!(SYSCALL_SESSION_INPUT_TRY_READ, 0x5059_0150);
        assert_eq!(SESSION_INPUT_RESULT_EVENT, 0x5059_004F);
        assert_eq!(SESSION_INPUT_RESULT_EMPTY, 0x5059_0150_0000);
        assert_eq!(SESSION_INPUT_RESOURCE_ID, 0x1A50_0100);
        assert_eq!(core::mem::size_of::<SessionInputEventV1>(), 40);
        assert_eq!(core::mem::align_of::<SessionInputEventV1>(), 8);
        assert_eq!(core::mem::offset_of!(SessionInputEventV1, sequence), 0);
        assert_eq!(core::mem::offset_of!(SessionInputEventV1, kind), 8);
        assert_eq!(core::mem::offset_of!(SessionInputEventV1, source), 10);
        assert_eq!(core::mem::offset_of!(SessionInputEventV1, flags), 12);
        assert_eq!(core::mem::offset_of!(SessionInputEventV1, value0), 16);
        assert_eq!(core::mem::offset_of!(SessionInputEventV1, value1), 20);
        assert_eq!(core::mem::offset_of!(SessionInputEventV1, reserved0), 24);
        assert_eq!(core::mem::offset_of!(SessionInputEventV1, reserved1), 32);
        let event = SessionInputEventV1::empty();
        assert_eq!(event.reserved0, 0);
        assert_eq!(event.reserved1, 0);
    }

    #[test]
    fn logical_key_tags_are_stable_not_rust_discriminants() {
        assert_eq!(KEY_A, 0x0001);
        assert_eq!(KEY_Z, 0x001A);
        assert_eq!(KEY_DIGIT0, 0x0020);
        assert_eq!(KEY_DIGIT9, 0x0029);
        assert_eq!(KEY_ENTER, 0x0030);
        assert_eq!(KEY_ESCAPE, 0x0031);
        assert_eq!(KEY_SPACE, 0x0032);
        assert_eq!(KEY_BACKSPACE, 0x0033);
    }
}
