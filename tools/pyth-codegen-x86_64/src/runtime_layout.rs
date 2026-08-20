use pythos_shared::pyth_runtime_abi::{
    GRAPH_EXIT_BUDGET_EXHAUSTED, GRAPH_EXIT_OK, GRAPH_EXIT_RUNTIME_ERROR, GRAPH_RESULT_UNIT,
    GraphExitRecord, PYTH_GRAPH_BOOTSTRAP_MAGIC, PYTH_GRAPH_RUNTIME_ABI_MAJOR,
    PYTH_GRAPH_RUNTIME_ABI_MINOR, PythGraphBootstrapBlock, SYSCALL_PYTH_GRAPH_EXIT,
};

pub const BOOTSTRAP_MAGIC: u64 = PYTH_GRAPH_BOOTSTRAP_MAGIC;
pub const BOOTSTRAP_ABI_WORD: u64 =
    (PYTH_GRAPH_RUNTIME_ABI_MAJOR as u64) | ((PYTH_GRAPH_RUNTIME_ABI_MINOR as u64) << 16);
pub const BOOTSTRAP_ABI_MASK: u64 = 0xFFFF_FFFF;
pub const BOOTSTRAP_MAGIC_OFFSET: i32 =
    core::mem::offset_of!(PythGraphBootstrapBlock, magic) as i32;
pub const BOOTSTRAP_ABI_OFFSET: i32 =
    core::mem::offset_of!(PythGraphBootstrapBlock, abi_major) as i32;
pub const BOOTSTRAP_BUDGET_OFFSET: i32 =
    core::mem::offset_of!(PythGraphBootstrapBlock, instruction_budget) as i32;
pub const BOOTSTRAP_RESULT_PTR_OFFSET: i32 =
    core::mem::offset_of!(PythGraphBootstrapBlock, result_ptr) as i32;
pub const BOOTSTRAP_PACKAGE_PTR_OFFSET: i32 =
    core::mem::offset_of!(PythGraphBootstrapBlock, package_ptr) as i32;
pub const BOOTSTRAP_IMPORTS_OFFSET: i32 =
    core::mem::offset_of!(PythGraphBootstrapBlock, imports) as i32;

pub const CAPABILITY_BINDING_SIZE: usize =
    core::mem::size_of::<pythos_shared::pyth_runtime_abi::PythGraphCapabilityBinding>();
pub const CAPABILITY_BINDING_CAPABILITY_OFFSET: usize = core::mem::offset_of!(
    pythos_shared::pyth_runtime_abi::PythGraphCapabilityBinding,
    capability
);

pub const GRAPH_EXIT_RECORD_BYTES: usize = core::mem::size_of::<GraphExitRecord>();
pub const GRAPH_EXIT_OK_STATUS: u16 = GRAPH_EXIT_OK;
pub const GRAPH_EXIT_RUNTIME_ERROR_STATUS: u16 = GRAPH_EXIT_RUNTIME_ERROR;
pub const GRAPH_EXIT_BUDGET_EXHAUSTED_STATUS: u16 = GRAPH_EXIT_BUDGET_EXHAUSTED;
pub const GRAPH_EXIT_RESULT_UNIT: u16 = GRAPH_RESULT_UNIT;
pub const GRAPH_EXIT_SYSCALL: u64 = SYSCALL_PYTH_GRAPH_EXIT;

pub const RUNTIME_ERROR_BUDGET_EXHAUSTED: u16 = 1;
pub const RUNTIME_ERROR_UNSUPPORTED_OPCODE: u16 = 8;
