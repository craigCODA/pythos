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

impl Opcode {
    pub const fn code(self) -> u16 {
        self as u16
    }
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
