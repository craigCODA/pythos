#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum PythType {
    Unit = 0x0000,
    Bool = 0x0001,
    U64 = 0x0002,
    I64 = 0x0003,
    Bytes = 0x0004,
    Utf8 = 0x0005,
    ObjectId = 0x0006,
    RevisionId = 0x0007,
    TaskId = 0x0008,
    ProposalId = 0x0009,
    Capability = 0x000A,
    Effect = 0x000B,
    ErrorCode = 0x000C,
}

impl PythType {
    pub const fn code(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownType {
    pub code: u16,
}

impl TryFrom<u16> for PythType {
    type Error = UnknownType;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0x0000 => Ok(Self::Unit),
            0x0001 => Ok(Self::Bool),
            0x0002 => Ok(Self::U64),
            0x0003 => Ok(Self::I64),
            0x0004 => Ok(Self::Bytes),
            0x0005 => Ok(Self::Utf8),
            0x0006 => Ok(Self::ObjectId),
            0x0007 => Ok(Self::RevisionId),
            0x0008 => Ok(Self::TaskId),
            0x0009 => Ok(Self::ProposalId),
            0x000A => Ok(Self::Capability),
            0x000B => Ok(Self::Effect),
            0x000C => Ok(Self::ErrorCode),
            code => Err(UnknownType { code }),
        }
    }
}
