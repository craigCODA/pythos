//! Private encodings for the ADR 0110 proof.

pub const PROGRAM_LEN: usize = 64;
pub const MAX_PROGRAM_ADDRESSES: usize = 8;
pub const EVENT_LEN: usize = 24;
pub const INVOCATION_LEN: usize = 24 + PROGRAM_LEN + EVENT_LEN;
pub const OUTCOME_LEN: usize = 32;
pub const PRINCIPAL: u64 = 0x5059_5650_554c_0001;

const ADD_EVENT_VALUE: u8 = 1;
const BRANCH_ON_KIND: u8 = 2;
const WAIT_FOR_KIND: u8 = 3;
const EVALUATOR_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidPulse {
    Program,
    Event,
    Invocation,
    Outcome,
    Arithmetic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum EventKind {
    Task = 1,
    Message = 2,
    System = 3,
}

impl EventKind {
    fn decode(code: u8) -> Result<Self, InvalidPulse> {
        match code {
            1 => Ok(Self::Task),
            2 => Ok(Self::Message),
            3 => Ok(Self::System),
            _ => Err(InvalidPulse::Event),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Event {
    pub source: u32,
    pub kind: EventKind,
    pub id: u64,
    pub value: u64,
}

impl Event {
    pub fn encode(self) -> [u8; EVENT_LEN] {
        let mut bytes = [0; EVENT_LEN];
        bytes[..4].copy_from_slice(&self.source.to_le_bytes());
        bytes[4] = self.kind as u8;
        bytes[8..16].copy_from_slice(&self.id.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.value.to_le_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, InvalidPulse> {
        if bytes.len() != EVENT_LEN || bytes[5..8] != [0; 3] {
            return Err(InvalidPulse::Event);
        }
        let source = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let id = read_u64(bytes, 8);
        if source == 0 || id == 0 {
            return Err(InvalidPulse::Event);
        }
        Ok(Self {
            source,
            id,
            kind: EventKind::decode(bytes[4])?,
            value: read_u64(bytes, 16),
        })
    }

    pub fn same_id(self, other: Self) -> bool {
        self.source == other.source && self.id == other.id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Program([u8; PROGRAM_LEN]);

/// Read-only description of one address in a validated proof program.
#[derive(Clone, Copy)]
pub enum Instruction {
    Add { next: u8 },
    Branch { on: EventKind, yes: u8, no: u8 },
    Wait { for_kind: EventKind, next: u8 },
}

impl Program {
    pub fn decode(bytes: [u8; PROGRAM_LEN]) -> Result<Self, InvalidPulse> {
        let program = Self(bytes);
        let count = bytes[12];
        if &bytes[..8] != b"PYVPRG01"
            || bytes[8..10] != EVALUATOR_VERSION.to_le_bytes()
            || !(1..=8).contains(&program.budget())
            || !(1..=MAX_PROGRAM_ADDRESSES).contains(&usize::from(count))
            || program.entry() >= count
            || bytes[13..16] != [0; 3]
            || program.initial_x() > program.goal()
            || bytes[32 + usize::from(count) * 4..]
                .iter()
                .any(|byte| *byte != 0)
        {
            return Err(InvalidPulse::Program);
        }
        for cursor in 0..count {
            let [operation, argument, next, alternative] = program.link(cursor);
            if next >= count {
                return Err(InvalidPulse::Program);
            }
            let valid = match operation {
                ADD_EVENT_VALUE => argument == 0 && alternative == 0,
                BRANCH_ON_KIND => EventKind::decode(argument).is_ok() && alternative < count,
                WAIT_FOR_KIND => EventKind::decode(argument).is_ok() && alternative == 0,
                _ => false,
            };
            if !valid {
                return Err(InvalidPulse::Program);
            }
        }
        Ok(program)
    }

    pub fn bytes(&self) -> &[u8; PROGRAM_LEN] {
        &self.0
    }

    pub fn entry(self) -> u8 {
        self.0[11]
    }

    pub fn budget(self) -> u8 {
        self.0[10]
    }

    pub fn initial_x(self) -> u64 {
        read_u64(&self.0, 16)
    }

    pub fn goal(self) -> u64 {
        read_u64(&self.0, 24)
    }

    pub fn has_address(self, cursor: u8) -> bool {
        cursor < self.0[12]
    }

    pub fn wait_kind(self, cursor: u8) -> Option<EventKind> {
        if !self.has_address(cursor) {
            return None;
        }
        let [op, kind, ..] = self.link(cursor);
        if op == WAIT_FOR_KIND {
            EventKind::decode(kind).ok()
        } else {
            None
        }
    }

    pub fn instruction(self, cursor: u8) -> Option<Instruction> {
        if !self.has_address(cursor) {
            return None;
        }
        let [operation, argument, next, alternative] = self.link(cursor);
        match operation {
            ADD_EVENT_VALUE => Some(Instruction::Add { next }),
            BRANCH_ON_KIND => Some(Instruction::Branch {
                on: EventKind::decode(argument).ok()?,
                yes: next,
                no: alternative,
            }),
            WAIT_FOR_KIND => Some(Instruction::Wait {
                for_kind: EventKind::decode(argument).ok()?,
                next,
            }),
            _ => None,
        }
    }

    fn link(self, cursor: u8) -> [u8; 4] {
        let offset = 32 + usize::from(cursor) * 4;
        self.0[offset..offset + 4].try_into().unwrap()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Status {
    Active = 1,
    Waiting = 2,
    Finished = 3,
}

impl Status {
    pub fn decode(code: u8) -> Result<Self, InvalidPulse> {
        match code {
            1 => Ok(Self::Active),
            2 => Ok(Self::Waiting),
            3 => Ok(Self::Finished),
            _ => Err(InvalidPulse::Outcome),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Invocation {
    pub generation: u64,
    pub x: u64,
    pub cursor: u8,
    pub program: Program,
    pub event: Event,
}

impl Invocation {
    pub fn encode(self) -> [u8; INVOCATION_LEN] {
        let mut bytes = [0; INVOCATION_LEN];
        bytes[..8].copy_from_slice(&self.generation.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.x.to_le_bytes());
        bytes[16] = self.cursor;
        bytes[24..24 + PROGRAM_LEN].copy_from_slice(self.program.bytes());
        bytes[24 + PROGRAM_LEN..].copy_from_slice(&self.event.encode());
        bytes
    }

    pub fn decode(bytes: &[u8; INVOCATION_LEN]) -> Result<Self, InvalidPulse> {
        if bytes[17..24] != [0; 7] {
            return Err(InvalidPulse::Invocation);
        }
        let input = Self {
            generation: read_u64(bytes, 0),
            x: read_u64(bytes, 8),
            cursor: bytes[16],
            program: Program::decode(bytes[24..24 + PROGRAM_LEN].try_into().unwrap())?,
            event: Event::decode(&bytes[24 + PROGRAM_LEN..])?,
        };
        input.validate()?;
        Ok(input)
    }

    fn validate(self) -> Result<(), InvalidPulse> {
        if self.generation == 0
            || !self.program.has_address(self.cursor)
            || self.x > self.program.goal()
        {
            return Err(InvalidPulse::Invocation);
        }
        Ok(())
    }

    pub fn execute(self) -> Result<Outcome, InvalidPulse> {
        self.validate()?;
        let mut x = self.x;
        let mut cursor = self.cursor;
        let mut steps = 0;
        let status = loop {
            if x == self.program.goal() {
                break Status::Finished;
            }
            if steps == self.program.budget() {
                break Status::Active;
            }
            let [operation, argument, next, alternative] = self.program.link(cursor);
            let matches_kind = self.event.kind as u8 == argument;
            cursor = match operation {
                ADD_EVENT_VALUE => {
                    x = x
                        .checked_add(self.event.value)
                        .filter(|value| *value <= self.program.goal())
                        .ok_or(InvalidPulse::Arithmetic)?;
                    next
                }
                BRANCH_ON_KIND if !matches_kind => alternative,
                WAIT_FOR_KIND if !matches_kind => break Status::Waiting,
                _ => next,
            };
            steps += 1;
        };
        Ok(Outcome {
            generation: self.generation,
            x,
            cursor,
            status,
            steps,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub generation: u64,
    pub x: u64,
    pub cursor: u8,
    pub status: Status,
    pub steps: u8,
}

pub fn arithmetic_failure(generation: u64) -> [u8; OUTCOME_LEN] {
    let mut bytes = [0; OUTCOME_LEN];
    bytes[..8].copy_from_slice(b"PYVERR01");
    bytes[8..16].copy_from_slice(&generation.to_le_bytes());
    bytes
}

impl Outcome {
    pub fn encode(self) -> [u8; OUTCOME_LEN] {
        let mut bytes = [0; OUTCOME_LEN];
        bytes[..8].copy_from_slice(b"PYVOUT01");
        bytes[8..16].copy_from_slice(&self.generation.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.x.to_le_bytes());
        bytes[24..27].copy_from_slice(&[self.cursor, self.status as u8, self.steps]);
        bytes
    }

    pub fn decode(bytes: &[u8; OUTCOME_LEN]) -> Result<Self, InvalidPulse> {
        if &bytes[..8] != b"PYVOUT01" || bytes[27..] != [0; 5] {
            return Err(InvalidPulse::Outcome);
        }
        Ok(Self {
            generation: read_u64(bytes, 8),
            x: read_u64(bytes, 16),
            cursor: bytes[24],
            status: Status::decode(bytes[25])?,
            steps: bytes[26],
        })
    }

    pub fn validate_for(self, input: Invocation) -> Result<(), InvalidPulse> {
        input.validate()?;
        if self.generation != input.generation
            || self.x < input.x
            || self.x > input.program.goal()
            || !input.program.has_address(self.cursor)
            || self.steps > input.program.budget()
            || (self.steps == 0 && (self.x != input.x || self.cursor != input.cursor))
        {
            return Err(InvalidPulse::Outcome);
        }
        let at_goal = self.x == input.program.goal();
        let valid = match self.status {
            Status::Finished => at_goal,
            Status::Active => !at_goal && self.steps == input.program.budget(),
            Status::Waiting => {
                !at_goal
                    && self.steps < input.program.budget()
                    && input
                        .program
                        .wait_kind(self.cursor)
                        .is_some_and(|kind| kind != input.event.kind)
            }
        };
        if valid {
            Ok(())
        } else {
            Err(InvalidPulse::Outcome)
        }
    }
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(initial: u64, goal: u64, links: &[[u8; 4]]) -> [u8; 64] {
        let mut bytes = [0; 64];
        bytes[..8].copy_from_slice(b"PYVPRG01");
        bytes[8..10].copy_from_slice(&1u16.to_le_bytes());
        bytes[10] = 2;
        bytes[12] = links.len() as u8;
        bytes[16..24].copy_from_slice(&initial.to_le_bytes());
        bytes[24..32].copy_from_slice(&goal.to_le_bytes());
        for (slot, link) in bytes[32..].chunks_exact_mut(4).zip(links) {
            slot.copy_from_slice(link);
        }
        bytes
    }

    fn event(kind: EventKind, value: u64) -> Event {
        Event {
            source: 1,
            id: 9,
            kind,
            value,
        }
    }

    #[test]
    fn resumes_the_same_event_at_the_saved_address() {
        let program = Program::decode(program(0, 11, &[[1, 0, 0, 0]])).unwrap();
        let mut input = Invocation {
            generation: 3,
            x: 4,
            cursor: 0,
            program,
            event: event(EventKind::Task, 1),
        };
        let resumed = Invocation::decode(&input.encode())
            .unwrap()
            .execute()
            .unwrap();
        assert_eq!(
            resumed,
            Outcome {
                generation: 3,
                x: 6,
                cursor: 0,
                status: Status::Active,
                steps: 2
            }
        );
        input.x = 10;
        assert_eq!(input.execute().unwrap().x, 11);
        assert_eq!(input.execute().unwrap().status, Status::Finished);
        assert_eq!(input.execute().unwrap().steps, 1);
    }

    #[test]
    fn follows_a_typed_branch_then_suspends_at_an_unmet_event_condition() {
        let program = Program::decode(program(
            0,
            1,
            &[[2, 1, 1, 3], [3, 2, 2, 0], [1, 0, 2, 0], [3, 3, 3, 0]],
        ))
        .unwrap();
        let mut input = Invocation {
            generation: 1,
            x: 0,
            cursor: 0,
            program,
            event: event(EventKind::Task, 1),
        };
        let result = input.execute().unwrap();
        assert_eq!(
            (result.x, result.cursor, result.status, result.steps),
            (0, 1, Status::Waiting, 1)
        );
        input.cursor = result.cursor;
        input.event = event(EventKind::Message, 1);
        assert_eq!(input.execute().unwrap().status, Status::Finished);
        input.cursor = program.entry();
        let restarted = input.execute().unwrap();
        assert_eq!((restarted.status, restarted.cursor), (Status::Waiting, 3));
    }

    #[test]
    fn fails_on_an_out_of_bounds_link_or_overflow() {
        assert!(Program::decode(program(0, 11, &[[1, 0, 1, 0]])).is_err());
        let program = Program::decode(program(u64::MAX - 1, u64::MAX, &[[1, 0, 0, 0]])).unwrap();
        let input = Invocation {
            generation: 1,
            x: u64::MAX - 1,
            cursor: 0,
            program,
            event: event(EventKind::Task, 2),
        };
        assert!(input.execute().is_err());
    }

    #[test]
    fn rejects_a_stale_outcome_before_publication() {
        let program = Program::decode(program(0, 11, &[[1, 0, 0, 0]])).unwrap();
        let input = Invocation {
            generation: 2,
            x: 2,
            cursor: 0,
            program,
            event: event(EventKind::Task, 1),
        };
        let stale = Outcome {
            generation: 1,
            x: 4,
            cursor: 0,
            status: Status::Active,
            steps: 2,
        };
        assert_eq!(Outcome::decode(&stale.encode()).unwrap(), stale);
        assert!(stale.validate_for(input).is_err());
    }
}
