//! Versioned messages for the opt-in native world session.

mod edit;

use crate::session_input_abi::{
    KEY_B, KEY_DIGIT1, KEY_DIGIT2, KEY_DIGIT3, KEY_DIGIT4, KEY_F, KEY_H, KEY_M, KEY_X,
    SESSION_INPUT_FLAG_GAP_BEFORE, SESSION_INPUT_KIND_KEY_DOWN,
    SESSION_INPUT_KIND_MOUSE_BUTTON_STATE, SESSION_INPUT_KIND_RELATIVE_MOTION,
    SESSION_INPUT_SOURCE_KEYBOARD, SESSION_INPUT_SOURCE_MOUSE, SessionInputEventV1,
};

pub const PRINCIPAL: u64 = 0x5059_574F_524C_0001;
pub const HALF_WIDTH_MM: i32 = 22_491;
pub const OBJECT_LIMIT: usize = 12;
const POSE_LEN: usize = 12;
const HEADER_LEN: usize = 56;
const OBJECT_LEN: usize = 24;
pub const WORLD_LEN: usize = HEADER_LEN + OBJECT_LEN * OBJECT_LIMIT;
const VIEW_LEN: usize = 24;
pub const FRAME_LEN: usize = WORLD_LEN + VIEW_LEN;
pub const INVOCATION_LEN: usize = FRAME_LEN + 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidWorld;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Pose {
    pub x: i32,
    pub z: i32,
    pub heading: u8,
}

impl Pose {
    pub const ORIGIN: Self = Self {
        x: 0,
        z: 0,
        heading: 0,
    };

    pub fn inside(self) -> bool {
        (-HALF_WIDTH_MM..=HALF_WIDTH_MM).contains(&self.x)
            && (-HALF_WIDTH_MM..=HALF_WIDTH_MM).contains(&self.z)
            && self.heading < 16
    }

    /// Sine and cosine of the heading at 22.5 degree increments, scaled by 1024.
    pub fn direction(self) -> (i32, i32) {
        const DIRECTIONS: [(i32, i32); 16] = [
            (0, 1024),
            (392, 946),
            (724, 724),
            (946, 392),
            (1024, 0),
            (946, -392),
            (724, -724),
            (392, -946),
            (0, -1024),
            (-392, -946),
            (-724, -724),
            (-946, -392),
            (-1024, 0),
            (-946, 392),
            (-724, 724),
            (-392, 946),
        ];
        DIRECTIONS[usize::from(self.heading & 15)]
    }

    pub fn offset(self, forward_mm: i32, right_mm: i32) -> Self {
        let (sin, cos) = self.direction();
        Self {
            x: self.x + (forward_mm * sin + right_mm * cos) / 1024,
            z: self.z + (-forward_mm * cos + right_mm * sin) / 1024,
            ..self
        }
    }

    fn encode(self, bytes: &mut [u8]) {
        bytes[..4].copy_from_slice(&self.x.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.z.to_le_bytes());
        bytes[8] = self.heading;
    }

    fn decode(bytes: &[u8]) -> Result<Self, InvalidWorld> {
        let pose = Self {
            x: i32::from_le_bytes(bytes[..4].try_into().unwrap()),
            z: i32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            heading: bytes[8],
        };
        if pose.inside() && bytes[9..POSE_LEN] == [0; 3] {
            Ok(pose)
        } else {
            Err(InvalidWorld)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Kind {
    Empty,
    Prop,
    Screen,
    Anchor,
}

impl Kind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::Prop => "PROP",
            Self::Screen => "SCREEN",
            Self::Anchor => "ANCHOR",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Object {
    pub id: u32,
    pub kind: Kind,
    pub pose: Pose,
    pub target: u32,
}

impl Object {
    pub const EMPTY: Self = Self {
        id: 0,
        kind: Kind::Empty,
        pose: Pose::ORIGIN,
        target: 0,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Edit {
    None,
    Place,
    Template,
    Move,
    Remove,
    Bind,
    Use,
    Arrival,
}

impl Edit {
    const VALUES: [Self; 8] = [
        Self::None,
        Self::Place,
        Self::Template,
        Self::Move,
        Self::Remove,
        Self::Bind,
        Self::Use,
        Self::Arrival,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Place => "place",
            Self::Template => "template",
            Self::Move => "move",
            Self::Remove => "remove",
            Self::Bind => "bind",
            Self::Use => "use",
            Self::Arrival => "arrival",
        }
    }

    fn from_event(input: SessionInputEventV1) -> Option<Self> {
        if input.kind != SESSION_INPUT_KIND_KEY_DOWN || input.flags != 0 {
            return None;
        }
        match u16::try_from(input.value0).ok()? {
            KEY_DIGIT1 | KEY_DIGIT2 | KEY_DIGIT3 => Some(Self::Place),
            KEY_DIGIT4 => Some(Self::Template),
            KEY_M => Some(Self::Move),
            KEY_X => Some(Self::Remove),
            KEY_B => Some(Self::Bind),
            KEY_F => Some(Self::Use),
            KEY_H => Some(Self::Arrival),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Receipt {
    pub edit: Edit,
    pub sequence: u64,
    pub actor: u32,
    pub target: u32,
}

impl Receipt {
    const EMPTY: Self = Self {
        edit: Edit::None,
        sequence: 0,
        actor: 0,
        target: 0,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct World {
    pub revision: u64,
    pub next_id: u32,
    pub arrival: Pose,
    pub focus: u32,
    pub slots: [Object; OBJECT_LIMIT],
    pub receipt: Receipt,
}

impl World {
    pub const fn blank() -> Self {
        Self {
            revision: 0,
            next_id: 1,
            arrival: Pose::ORIGIN,
            focus: 0,
            slots: [Object::EMPTY; OBJECT_LIMIT],
            receipt: Receipt::EMPTY,
        }
    }

    pub fn objects(&self) -> impl Iterator<Item = &Object> + Clone {
        self.slots.iter().filter(|object| object.id != 0)
    }

    pub fn object(&self, id: u32) -> Option<&Object> {
        self.objects().find(|object| object.id == id)
    }

    pub fn object_mut(&mut self, id: u32) -> Option<&mut Object> {
        self.slots
            .iter_mut()
            .find(|object| id != 0 && object.id == id)
    }

    pub fn encode(self) -> [u8; WORLD_LEN] {
        let mut bytes = [0; WORLD_LEN];
        bytes[..8].copy_from_slice(b"PYWSTA01");
        bytes[8..16].copy_from_slice(&self.revision.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.next_id.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.focus.to_le_bytes());
        self.arrival.encode(&mut bytes[24..36]);
        bytes[36..40].copy_from_slice(&self.receipt.actor.to_le_bytes());
        bytes[40..44].copy_from_slice(&self.receipt.target.to_le_bytes());
        bytes[44] = self.receipt.edit as u8;
        bytes[48..56].copy_from_slice(&self.receipt.sequence.to_le_bytes());
        for (object, encoded) in self
            .slots
            .iter()
            .zip(bytes[HEADER_LEN..].chunks_exact_mut(OBJECT_LEN))
        {
            encoded[..4].copy_from_slice(&object.id.to_le_bytes());
            encoded[4] = object.kind as u8;
            object.pose.encode(&mut encoded[8..20]);
            encoded[20..24].copy_from_slice(&object.target.to_le_bytes());
        }
        bytes
    }

    pub fn decode(bytes: &[u8; WORLD_LEN]) -> Result<Self, InvalidWorld> {
        if &bytes[..8] != b"PYWSTA01" || bytes[45..48] != [0; 3] {
            return Err(InvalidWorld);
        }
        let mut world = Self {
            revision: read_u64(bytes, 8),
            next_id: read_u32(bytes, 16),
            arrival: Pose::decode(&bytes[24..36])?,
            focus: read_u32(bytes, 20),
            slots: [Object::EMPTY; OBJECT_LIMIT],
            receipt: Receipt {
                edit: *Edit::VALUES
                    .get(usize::from(bytes[44]))
                    .ok_or(InvalidWorld)?,
                sequence: read_u64(bytes, 48),
                actor: read_u32(bytes, 36),
                target: read_u32(bytes, 40),
            },
        };
        for (index, encoded) in bytes[HEADER_LEN..].chunks_exact(OBJECT_LEN).enumerate() {
            let kind = match encoded[4] {
                0 => Kind::Empty,
                1 => Kind::Prop,
                2 => Kind::Screen,
                3 => Kind::Anchor,
                _ => return Err(InvalidWorld),
            };
            let object = Object {
                id: read_u32(encoded, 0),
                kind,
                pose: Pose::decode(&encoded[8..20])?,
                target: read_u32(encoded, 20),
            };
            if encoded[5..8] != [0; 3]
                || if object.id == 0 {
                    object != Object::EMPTY
                } else {
                    object.kind == Kind::Empty
                        || object.id >= world.next_id
                        || world.slots[..index].iter().any(|slot| slot.id == object.id)
                }
            {
                return Err(InvalidWorld);
            }
            world.slots[index] = object;
        }
        if world.next_id == 0
            || !world.has_reference(world.focus)
            || world
                .objects()
                .any(|object| !world.has_reference(object.target))
            || if world.revision == 0 {
                world != Self::blank()
            } else {
                world.receipt.edit == Edit::None
                    || world.receipt.actor >= world.next_id
                    || world.receipt.target >= world.next_id
            }
        {
            return Err(InvalidWorld);
        }
        Ok(world)
    }

    fn has_reference(&self, id: u32) -> bool {
        id == 0 || self.object(id).is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Notice {
    Ready,
    Saved,
    Focused,
    Outside,
    Select,
    ChooseTarget,
    NoJob,
    Full,
    ToAnchor,
    InputGap,
    Exit,
}

impl Notice {
    const VALUES: [Self; 11] = [
        Self::Ready,
        Self::Saved,
        Self::Focused,
        Self::Outside,
        Self::Select,
        Self::ChooseTarget,
        Self::NoJob,
        Self::Full,
        Self::ToAnchor,
        Self::InputGap,
        Self::Exit,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Saved => "Saved",
            Self::Focused => "Job completed",
            Self::Outside => "Choose a spot within the ground",
            Self::Select => "Select an object",
            Self::ChooseTarget => "Choose a job target",
            Self::NoJob => "Give the selected object a job",
            Self::Full => "World object capacity reached",
            Self::ToAnchor => "At standing anchor",
            Self::InputGap => "Input gap cleared selection",
            Self::Exit => "World saved",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct View {
    pub pose: Pose,
    pub selected: u32,
    pub target: u32,
    pub notice: Notice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frame {
    pub world: World,
    pub view: View,
}

impl Frame {
    pub fn restored(world: World) -> Self {
        Self {
            world,
            view: View {
                pose: world.arrival,
                selected: 0,
                target: 0,
                notice: Notice::Ready,
            },
        }
    }

    pub fn encode(self) -> [u8; FRAME_LEN] {
        let mut bytes = [0; FRAME_LEN];
        bytes[..WORLD_LEN].copy_from_slice(&self.world.encode());
        let view = &mut bytes[WORLD_LEN..];
        self.view.pose.encode(&mut view[..12]);
        view[12..16].copy_from_slice(&self.view.selected.to_le_bytes());
        view[16..20].copy_from_slice(&self.view.target.to_le_bytes());
        view[20] = self.view.notice as u8;
        bytes
    }

    pub fn decode(bytes: &[u8; FRAME_LEN]) -> Result<Self, InvalidWorld> {
        let world = World::decode(bytes[..WORLD_LEN].try_into().unwrap())?;
        let view = &bytes[WORLD_LEN..];
        let selected = read_u32(view, 12);
        let target = read_u32(view, 16);
        if view[21..] != [0; 3] || !world.has_reference(selected) || !world.has_reference(target) {
            return Err(InvalidWorld);
        }
        Ok(Self {
            world,
            view: View {
                pose: Pose::decode(&view[..12])?,
                selected,
                target,
                notice: *Notice::VALUES
                    .get(usize::from(view[20]))
                    .ok_or(InvalidWorld)?,
            },
        })
    }

    pub fn validate_result(self, input: Invocation) -> Result<Self, InvalidWorld> {
        let previous = input.frame.world;
        let expected = previous
            .edited_for(input.frame.view, input.event)
            .and_then(Result::ok)
            .unwrap_or(previous);
        if self.world == expected {
            Ok(self)
        } else {
            Err(InvalidWorld)
        }
    }
}

#[derive(Clone, Copy)]
pub struct Invocation {
    pub frame: Frame,
    pub event: SessionInputEventV1,
}

impl Invocation {
    pub fn encode(self) -> [u8; INVOCATION_LEN] {
        let mut bytes = [0; INVOCATION_LEN];
        bytes[..FRAME_LEN].copy_from_slice(&self.frame.encode());
        let event = &mut bytes[FRAME_LEN..];
        event[..8].copy_from_slice(&self.event.sequence.to_le_bytes());
        event[8..10].copy_from_slice(&self.event.kind.to_le_bytes());
        event[10..12].copy_from_slice(&self.event.source.to_le_bytes());
        event[12..16].copy_from_slice(&self.event.flags.to_le_bytes());
        event[16..20].copy_from_slice(&self.event.value0.to_le_bytes());
        event[20..24].copy_from_slice(&self.event.value1.to_le_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8; INVOCATION_LEN]) -> Result<Self, InvalidWorld> {
        let frame = Frame::decode(bytes[..FRAME_LEN].try_into().unwrap())?;
        let event = &bytes[FRAME_LEN..];
        let event = SessionInputEventV1 {
            sequence: read_u64(event, 0),
            kind: u16::from_le_bytes(event[8..10].try_into().unwrap()),
            source: u16::from_le_bytes(event[10..12].try_into().unwrap()),
            flags: read_u32(event, 12),
            value0: i32::from_le_bytes(event[16..20].try_into().unwrap()),
            value1: i32::from_le_bytes(event[20..24].try_into().unwrap()),
            reserved0: read_u64(event, 24),
            reserved1: read_u64(event, 32),
        };
        let valid_source = matches!(
            (event.kind, event.source),
            (SESSION_INPUT_KIND_KEY_DOWN, SESSION_INPUT_SOURCE_KEYBOARD)
                | (
                    SESSION_INPUT_KIND_RELATIVE_MOTION | SESSION_INPUT_KIND_MOUSE_BUTTON_STATE,
                    SESSION_INPUT_SOURCE_MOUSE
                )
        );
        if !valid_source
            || event.flags & !SESSION_INPUT_FLAG_GAP_BEFORE != 0
            || event.reserved0 != 0
            || event.reserved1 != 0
        {
            return Err(InvalidWorld);
        }
        Ok(Self { frame, event })
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
