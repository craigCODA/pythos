use crate::pyth_tig::types::PythType;

pub const RESOURCE_SYSTEM_LOG: u16 = 1;
pub const RESOURCE_OBJECT_WORKSPACE: u16 = 2;
pub const RESOURCE_OBJECT: u16 = 3;
pub const RESOURCE_TASK: u16 = 4;
pub const RESOURCE_GRAPH: u16 = 5;
pub const RESOURCE_COMMAND: u16 = 6;

pub const RIGHTS_READ: u64 = 0x0001;
pub const RIGHTS_QUERY: u64 = 0x0002;
pub const RIGHTS_REVISE: u64 = 0x0004;
pub const RIGHTS_CREATE: u64 = 0x0008;
pub const RIGHTS_APPEND: u64 = 0x0010;
pub const RIGHTS_APPROVE: u64 = 0x0020;
pub const RIGHTS_CONTROL: u64 = 0x0040;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Opcode {
    BlockParam = 0x0001,
    ConstBool = 0x0002,
    ConstU64 = 0x0003,
    ConstI64 = 0x0004,
    ConstBytes = 0x0005,
    ConstUtf8 = 0x0006,
    EffectStart = 0x0007,
    HostResult = 0x0008,
    Eq = 0x0100,
    LessThanU64 = 0x0101,
    AddU64 = 0x0102,
    SubU64 = 0x0103,
    BoolAnd = 0x0104,
    BoolOr = 0x0105,
    BoolNot = 0x0106,
    Select = 0x0107,
    Jump = 0x0200,
    Branch = 0x0201,
    Return = 0x0202,
    SystemLog = 0x1000,
    ObjectCreate = 0x1100,
    ObjectQuery = 0x1101,
    ObjectInspect = 0x1102,
    ObjectRevise = 0x1103,
    ObjectHistory = 0x1104,
    TaskActiveRead = 0x1200,
    TaskProposalEmit = 0x1201,
    TaskProposalApprove = 0x1202,
    TaskSuspend = 0x1203,
    TaskRevive = 0x1204,
    TaskContextRead = 0x1205,
    GraphQueryRelated = 0x1300,
    RelevanceAssertionEmit = 0x1301,
    CapabilityRequestEmit = 0x1400,
    CommandRead = 0x1500,
    CommandResultEmit = 0x1501,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpcodeSignature {
    pub inputs: [PythType; 4],
    pub input_count: u8,
    pub result: PythType,
    pub effectful: bool,
    pub terminator: bool,
    pub required_resource_kind: Option<u16>,
    pub required_rights: u64,
}

impl Opcode {
    pub const fn code(self) -> u16 {
        self as u16
    }

    pub const fn signature(self) -> OpcodeSignature {
        use PythType::{
            Bool, Bytes, Effect, ErrorCode, I64, ObjectId, ProposalId, TaskId, U64, Unit, Utf8,
        };

        match self {
            Self::BlockParam => sig([], Unit),
            Self::ConstBool => sig([], Bool),
            Self::ConstU64 => sig([], U64),
            Self::ConstI64 => sig([], I64),
            Self::ConstBytes => sig([], Bytes),
            Self::ConstUtf8 => sig([], Utf8),
            Self::EffectStart => sig([], Effect),
            Self::HostResult => sig([Effect], Unit),
            Self::Eq => sig([U64, U64], Bool),
            Self::LessThanU64 => sig([U64, U64], Bool),
            Self::AddU64 => sig([U64, U64], U64),
            Self::SubU64 => sig([U64, U64], U64),
            Self::BoolAnd => sig([Bool, Bool], Bool),
            Self::BoolOr => sig([Bool, Bool], Bool),
            Self::BoolNot => sig([Bool], Bool),
            Self::Select => sig([Bool, U64, U64], U64),
            Self::Jump => terminator_sig(Unit),
            Self::Branch => terminator_with_inputs([Bool], Unit),
            Self::Return => terminator_sig(Unit),
            Self::SystemLog => host_sig([Effect, Utf8], RESOURCE_SYSTEM_LOG, RIGHTS_READ),
            Self::ObjectCreate => {
                host_sig([Effect, Utf8], RESOURCE_OBJECT_WORKSPACE, RIGHTS_CREATE)
            }
            Self::ObjectQuery => host_sig([Effect, Utf8], RESOURCE_OBJECT_WORKSPACE, RIGHTS_QUERY),
            Self::ObjectInspect => host_sig([Effect, ObjectId], RESOURCE_OBJECT, RIGHTS_READ),
            Self::ObjectRevise => {
                host_sig([Effect, ObjectId, Bytes], RESOURCE_OBJECT, RIGHTS_REVISE)
            }
            Self::ObjectHistory => host_sig([Effect, ObjectId], RESOURCE_OBJECT, RIGHTS_READ),
            Self::TaskActiveRead => host_sig([Effect], RESOURCE_TASK, RIGHTS_READ),
            Self::TaskProposalEmit => {
                host_sig([Effect, TaskId, U64, Utf8], RESOURCE_TASK, RIGHTS_APPEND)
            }
            Self::TaskProposalApprove => {
                host_sig([Effect, ProposalId], RESOURCE_TASK, RIGHTS_APPROVE)
            }
            Self::TaskSuspend | Self::TaskRevive => {
                host_sig([Effect, TaskId], RESOURCE_TASK, RIGHTS_CONTROL)
            }
            Self::TaskContextRead => host_sig([Effect, TaskId], RESOURCE_TASK, RIGHTS_READ),
            Self::GraphQueryRelated => host_sig([Effect, ObjectId], RESOURCE_GRAPH, RIGHTS_QUERY),
            Self::RelevanceAssertionEmit => host_sig(
                [Effect, ObjectId, ObjectId, U64],
                RESOURCE_GRAPH,
                RIGHTS_APPEND,
            ),
            Self::CapabilityRequestEmit => host_sig([Effect, Utf8], RESOURCE_TASK, RIGHTS_APPEND),
            Self::CommandRead => host_sig([Effect], RESOURCE_COMMAND, RIGHTS_READ),
            Self::CommandResultEmit => {
                host_sig([Effect, ErrorCode, Utf8], RESOURCE_COMMAND, RIGHTS_APPEND)
            }
        }
    }
}

const fn sig<const N: usize>(inputs: [PythType; N], result: PythType) -> OpcodeSignature {
    let mut padded = [PythType::Unit; 4];
    let mut index = 0usize;
    while index < N {
        padded[index] = inputs[index];
        index += 1;
    }

    OpcodeSignature {
        inputs: padded,
        input_count: N as u8,
        result,
        effectful: false,
        terminator: false,
        required_resource_kind: None,
        required_rights: 0,
    }
}

const fn terminator_sig(result: PythType) -> OpcodeSignature {
    let mut signature = sig([], result);
    signature.terminator = true;
    signature
}

const fn terminator_with_inputs<const N: usize>(
    inputs: [PythType; N],
    result: PythType,
) -> OpcodeSignature {
    let mut signature = sig(inputs, result);
    signature.terminator = true;
    signature
}

const fn host_sig<const N: usize>(
    inputs: [PythType; N],
    resource_kind: u16,
    rights: u64,
) -> OpcodeSignature {
    let mut signature = sig(inputs, PythType::Effect);
    signature.effectful = true;
    signature.required_resource_kind = Some(resource_kind);
    signature.required_rights = rights;
    signature
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownOpcode {
    pub code: u16,
}

impl TryFrom<u16> for Opcode {
    type Error = UnknownOpcode;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0x0001 => Ok(Self::BlockParam),
            0x0002 => Ok(Self::ConstBool),
            0x0003 => Ok(Self::ConstU64),
            0x0004 => Ok(Self::ConstI64),
            0x0005 => Ok(Self::ConstBytes),
            0x0006 => Ok(Self::ConstUtf8),
            0x0007 => Ok(Self::EffectStart),
            0x0008 => Ok(Self::HostResult),
            0x0100 => Ok(Self::Eq),
            0x0101 => Ok(Self::LessThanU64),
            0x0102 => Ok(Self::AddU64),
            0x0103 => Ok(Self::SubU64),
            0x0104 => Ok(Self::BoolAnd),
            0x0105 => Ok(Self::BoolOr),
            0x0106 => Ok(Self::BoolNot),
            0x0107 => Ok(Self::Select),
            0x0200 => Ok(Self::Jump),
            0x0201 => Ok(Self::Branch),
            0x0202 => Ok(Self::Return),
            0x1000 => Ok(Self::SystemLog),
            0x1100 => Ok(Self::ObjectCreate),
            0x1101 => Ok(Self::ObjectQuery),
            0x1102 => Ok(Self::ObjectInspect),
            0x1103 => Ok(Self::ObjectRevise),
            0x1104 => Ok(Self::ObjectHistory),
            0x1200 => Ok(Self::TaskActiveRead),
            0x1201 => Ok(Self::TaskProposalEmit),
            0x1202 => Ok(Self::TaskProposalApprove),
            0x1203 => Ok(Self::TaskSuspend),
            0x1204 => Ok(Self::TaskRevive),
            0x1205 => Ok(Self::TaskContextRead),
            0x1300 => Ok(Self::GraphQueryRelated),
            0x1301 => Ok(Self::RelevanceAssertionEmit),
            0x1400 => Ok(Self::CapabilityRequestEmit),
            0x1500 => Ok(Self::CommandRead),
            0x1501 => Ok(Self::CommandResultEmit),
            code => Err(UnknownOpcode { code }),
        }
    }
}
