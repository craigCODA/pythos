use pythos_shared::object_shell_abi::PackedCapability;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    U64(u64),
    I64(i64),
    Slice { offset: u32, len: u32, utf8: bool },
    ObjectId(u64),
    RevisionId(u64),
    TaskId(u64),
    ProposalId(u64),
    Capability(PackedCapability),
    Effect(u64),
    ErrorCode(u16),
}
