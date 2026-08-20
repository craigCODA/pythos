use pythos_shared::pyth_tig::verify::VerifiedGraph;

use crate::{CodegenError, Result};

pub const VALUE_SLOT_SIZE: usize = 16;
pub const VALUE_SLOT_TYPE_OFFSET: usize = 8;
pub const MAX_NATIVE_VALUE_FRAME_BYTES: usize = 12_288;

const SCRATCH_SLOT_COUNT: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLayout {
    value_slot_count: usize,
    value_bytes: usize,
    stack_frame_bytes: usize,
    budget_slot_offset: usize,
    executed_slot_offset: usize,
    last_node_slot_offset: usize,
}

impl NativeLayout {
    pub fn plan(graph: &VerifiedGraph<'_>) -> Result<Self> {
        let value_slot_count = graph.package().nodes().len();
        let value_bytes = value_slot_count
            .checked_mul(VALUE_SLOT_SIZE)
            .ok_or(CodegenError::AddressOverflow)?;
        if value_bytes > MAX_NATIVE_VALUE_FRAME_BYTES {
            return Err(CodegenError::StackFrameTooLarge {
                required: value_bytes,
                maximum: MAX_NATIVE_VALUE_FRAME_BYTES,
            });
        }

        let scratch_bytes = SCRATCH_SLOT_COUNT
            .checked_mul(VALUE_SLOT_SIZE)
            .ok_or(CodegenError::AddressOverflow)?;
        let stack_frame_bytes = align_16(
            value_bytes
                .checked_add(scratch_bytes)
                .ok_or(CodegenError::AddressOverflow)?,
        )?;

        Ok(Self {
            value_slot_count,
            value_bytes,
            stack_frame_bytes,
            budget_slot_offset: value_bytes,
            executed_slot_offset: value_bytes + VALUE_SLOT_SIZE,
            last_node_slot_offset: value_bytes + 2 * VALUE_SLOT_SIZE,
        })
    }

    pub const fn value_slot_count(&self) -> usize {
        self.value_slot_count
    }

    pub const fn value_bytes(&self) -> usize {
        self.value_bytes
    }

    pub const fn stack_frame_bytes(&self) -> usize {
        self.stack_frame_bytes
    }

    pub const fn budget_slot_offset(&self) -> usize {
        self.budget_slot_offset
    }

    pub const fn executed_slot_offset(&self) -> usize {
        self.executed_slot_offset
    }

    pub const fn last_node_slot_offset(&self) -> usize {
        self.last_node_slot_offset
    }

    pub fn value_slot_offset(&self, node_index: usize) -> Result<usize> {
        if node_index >= self.value_slot_count {
            return Err(CodegenError::AddressOverflow);
        }
        node_index
            .checked_mul(VALUE_SLOT_SIZE)
            .ok_or(CodegenError::AddressOverflow)
    }
}

fn align_16(value: usize) -> Result<usize> {
    value
        .checked_add(15)
        .map(|value| value & !15)
        .ok_or(CodegenError::AddressOverflow)
}
