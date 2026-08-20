use pythos_shared::pyth_tig::{
    NO_VALUE, NodeRecord, format::PythGraphPackage, opcode::Opcode, types::PythType,
    verify::VerifiedGraph,
};

use crate::{
    CodegenError, Result,
    elf::ElfImage,
    layout::{NativeLayout, VALUE_SLOT_TYPE_OFFSET},
    patch::Label,
    runtime_layout,
    x86::{CodeBuffer, ConditionCode, Memory, Register},
};

const TEXT_BASE: u64 = 0x0040_0000;
const LABEL_INVALID_BOOTSTRAP: Label = Label::new(1);
const LABEL_BUDGET_EXIT: Label = Label::new(2);
const LABEL_RUNTIME_ERROR_EXIT: Label = Label::new(3);
const LABEL_DONE: Label = Label::new(4);
const LABEL_NODE_BUDGET_BASE: u32 = 10_000;
const LABEL_BLOCK_BASE: u32 = 20_000;
const LABEL_LOCAL_BASE: u32 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeImage {
    pub bytes: Vec<u8>,
    pub metadata: NativeMetadata,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NativeMetadata {
    pub executable_nodes: usize,
    pub budget_checks: usize,
    pub branch_patches: usize,
    pub block_parameter_moves: usize,
    pub value_slots: usize,
    pub stack_frame_bytes: usize,
}

pub fn lower_verified_graph(graph: VerifiedGraph<'_>) -> Result<NativeImage> {
    let layout = NativeLayout::plan(&graph)?;
    let lowerer = Lowerer::new(graph.package(), layout);
    lowerer.lower()
}

struct Lowerer<'a> {
    package: &'a PythGraphPackage<'a>,
    layout: NativeLayout,
    code: CodeBuffer,
    metadata: NativeMetadata,
    next_local_label: u32,
}

impl<'a> Lowerer<'a> {
    fn new(package: &'a PythGraphPackage<'a>, layout: NativeLayout) -> Self {
        Self {
            package,
            metadata: NativeMetadata {
                executable_nodes: package.nodes().len(),
                value_slots: layout.value_slot_count(),
                stack_frame_bytes: layout.stack_frame_bytes(),
                ..NativeMetadata::default()
            },
            layout,
            code: CodeBuffer::new(),
            next_local_label: LABEL_LOCAL_BASE,
        }
    }

    fn lower(mut self) -> Result<NativeImage> {
        self.emit_prologue()?;
        for (block_index, block) in self.package.blocks().iter().enumerate() {
            self.code.bind_label(block_label(block_index))?;
            let first =
                usize::try_from(block.first_node).map_err(|_| CodegenError::AddressOverflow)?;
            let count =
                usize::try_from(block.node_count).map_err(|_| CodegenError::AddressOverflow)?;
            let end = first
                .checked_add(count)
                .ok_or(CodegenError::AddressOverflow)?;
            for node_index in first..end {
                let node = self
                    .package
                    .nodes()
                    .get(node_index)
                    .ok_or(CodegenError::AddressOverflow)?;
                self.emit_budget_check(node_index)?;
                self.emit_node(node_index, node)?;
            }
        }
        self.emit_exits()?;
        self.code.resolve_labels()?;

        let data = vec![0; runtime_layout::GRAPH_EXIT_RECORD_BYTES];
        let bytes = ElfImage::new(TEXT_BASE)
            .with_text(self.code.bytes())
            .with_rodata(&[])
            .with_data(&data)
            .encode()?;

        Ok(NativeImage {
            bytes,
            metadata: self.metadata,
        })
    }

    fn emit_prologue(&mut self) -> Result<()> {
        self.code.push(Register::R12)?;
        self.code.push(Register::R13)?;
        self.code.push(Register::R14)?;
        self.code.push(Register::R15)?;
        self.code.mov_reg64(Register::R12, Register::Rdi)?;
        self.code.mov_reg64(Register::R15, Register::R12)?;

        self.code.mov_reg64_mem64(
            Register::Rax,
            Memory::base_disp32(Register::R15, runtime_layout::BOOTSTRAP_MAGIC_OFFSET),
        )?;
        self.code
            .mov_imm64(Register::Rbx, runtime_layout::BOOTSTRAP_MAGIC)?;
        self.code.cmp_reg64(Register::Rax, Register::Rbx)?;
        self.emit_jcc(ConditionCode::NotEqual, LABEL_INVALID_BOOTSTRAP)?;

        self.code.mov_reg64_mem64(
            Register::Rax,
            Memory::base_disp32(Register::R15, runtime_layout::BOOTSTRAP_ABI_OFFSET),
        )?;
        self.code
            .mov_imm64(Register::Rbx, runtime_layout::BOOTSTRAP_ABI_MASK)?;
        self.code.and_reg64(Register::Rax, Register::Rbx)?;
        self.code
            .mov_imm64(Register::Rbx, runtime_layout::BOOTSTRAP_ABI_WORD)?;
        self.code.cmp_reg64(Register::Rax, Register::Rbx)?;
        self.emit_jcc(ConditionCode::NotEqual, LABEL_INVALID_BOOTSTRAP)?;

        self.code.mov_reg64_mem64(
            Register::R14,
            Memory::base_disp32(Register::R15, runtime_layout::BOOTSTRAP_RESULT_PTR_OFFSET),
        )?;
        self.zero_exit_record()?;

        self.code
            .mov_imm64(Register::Rax, self.layout.stack_frame_bytes() as u64)?;
        self.code.sub_reg64(Register::Rsp, Register::Rax)?;
        self.code.mov_reg64(Register::R13, Register::Rsp)?;

        self.code.mov_reg64_mem64(
            Register::Rax,
            Memory::base_disp32(Register::R15, runtime_layout::BOOTSTRAP_BUDGET_OFFSET),
        )?;
        self.store_scratch(self.layout.budget_slot_offset(), Register::Rax)?;
        self.code.mov_imm64(Register::Rax, 0)?;
        self.store_scratch(self.layout.executed_slot_offset(), Register::Rax)?;
        self.store_scratch(self.layout.last_node_slot_offset(), Register::Rax)?;

        let entry = usize::try_from(self.package.header().entry_block)
            .map_err(|_| CodegenError::AddressOverflow)?;
        self.emit_jmp(block_label(entry))
    }

    fn zero_exit_record(&mut self) -> Result<()> {
        self.code.mov_imm64(Register::Rax, 0)?;
        for offset in [0, 8, 16, 24] {
            self.code
                .mov_mem64_reg64(Memory::base_disp8(Register::R14, offset), Register::Rax)?;
        }
        Ok(())
    }

    fn emit_budget_check(&mut self, node_index: usize) -> Result<()> {
        let exhausted = node_budget_label(node_index)?;
        self.load_scratch(Register::Rax, self.layout.budget_slot_offset())?;
        self.code.mov_imm64(Register::Rbx, 0)?;
        self.code.cmp_reg64(Register::Rax, Register::Rbx)?;
        self.emit_jcc(ConditionCode::Equal, exhausted)?;

        self.code.mov_imm64(Register::Rbx, 1)?;
        self.code.sub_reg64(Register::Rax, Register::Rbx)?;
        self.store_scratch(self.layout.budget_slot_offset(), Register::Rax)?;
        self.increment_scratch(self.layout.executed_slot_offset())?;
        self.code.mov_imm64(Register::Rax, node_index as u64)?;
        self.store_scratch(self.layout.last_node_slot_offset(), Register::Rax)?;
        self.metadata.budget_checks += 1;
        Ok(())
    }

    fn emit_node(&mut self, node_index: usize, node: NodeRecord) -> Result<()> {
        let opcode =
            Opcode::try_from(node.opcode).map_err(|_| CodegenError::UnsupportedOpcode {
                opcode: node.opcode,
            })?;
        match opcode {
            Opcode::BlockParam => self.emit_block_param(node_index, node),
            Opcode::ConstBool
            | Opcode::ConstU64
            | Opcode::ConstI64
            | Opcode::ConstBytes
            | Opcode::ConstUtf8
            | Opcode::EffectStart => self.emit_simple_value(node_index, node, opcode),
            Opcode::Eq
            | Opcode::LessThanU64
            | Opcode::AddU64
            | Opcode::SubU64
            | Opcode::BoolAnd
            | Opcode::BoolOr
            | Opcode::BoolNot
            | Opcode::Select => self.emit_pure_op(node_index, node, opcode),
            Opcode::Jump => self.emit_jump(node),
            Opcode::Branch => self.emit_branch(node),
            Opcode::Return => self.emit_return(node_index),
            _ => Err(CodegenError::UnsupportedOpcode {
                opcode: node.opcode,
            }),
        }
    }

    fn emit_block_param(&mut self, node_index: usize, node: NodeRecord) -> Result<()> {
        let result_type =
            PythType::try_from(node.result_type).map_err(|_| CodegenError::UnsupportedOpcode {
                opcode: node.opcode,
            })?;
        if result_type == PythType::Capability {
            let import_offset = usize::try_from(node.auxiliary0)
                .map_err(|_| CodegenError::AddressOverflow)?
                .checked_mul(runtime_layout::CAPABILITY_BINDING_SIZE)
                .and_then(|offset| {
                    offset.checked_add(runtime_layout::CAPABILITY_BINDING_CAPABILITY_OFFSET)
                })
                .and_then(|offset| {
                    offset.checked_add(runtime_layout::BOOTSTRAP_IMPORTS_OFFSET as usize)
                })
                .ok_or(CodegenError::AddressOverflow)?;
            self.code.mov_reg64_mem64(
                Register::Rax,
                Memory::base_disp32(Register::R15, i32_offset(import_offset)?),
            )?;
            self.store_value_from_register(node_index, result_type, Register::Rax)?;
        }
        Ok(())
    }

    fn emit_simple_value(
        &mut self,
        node_index: usize,
        node: NodeRecord,
        opcode: Opcode,
    ) -> Result<()> {
        let result_type =
            PythType::try_from(node.result_type).map_err(|_| CodegenError::UnsupportedOpcode {
                opcode: node.opcode,
            })?;
        let payload = match opcode {
            Opcode::ConstBool => node.immediate,
            Opcode::ConstU64 | Opcode::ConstI64 => node.immediate,
            Opcode::ConstBytes | Opcode::ConstUtf8 => {
                (u64::from(node.auxiliary1) << 32) | u64::from(node.auxiliary0)
            }
            Opcode::EffectStart => node_index as u64,
            Opcode::HostResult => 0,
            _ => 0,
        };
        self.store_typed_immediate(node_index, result_type, payload)
    }

    fn emit_pure_op(&mut self, node_index: usize, node: NodeRecord, opcode: Opcode) -> Result<()> {
        let result_type =
            PythType::try_from(node.result_type).map_err(|_| CodegenError::UnsupportedOpcode {
                opcode: node.opcode,
            })?;
        match opcode {
            Opcode::Eq => {
                self.load_value_payload(Register::Rax, node.input0)?;
                self.load_value_payload(Register::Rbx, node.input1)?;
                self.code.cmp_reg64(Register::Rax, Register::Rbx)?;
                self.code.setcc(ConditionCode::Equal, Register::R10)?;
                self.code.movzx_reg8(Register::Rax, Register::R10)?;
                self.store_value_from_register(node_index, result_type, Register::Rax)
            }
            Opcode::LessThanU64 => {
                self.load_value_payload(Register::Rax, node.input0)?;
                self.load_value_payload(Register::Rbx, node.input1)?;
                self.code.cmp_reg64(Register::Rax, Register::Rbx)?;
                self.code.setcc(ConditionCode::Below, Register::R10)?;
                self.code.movzx_reg8(Register::Rax, Register::R10)?;
                self.store_value_from_register(node_index, result_type, Register::Rax)
            }
            Opcode::AddU64 => self.emit_binary_reg_op(node_index, node, result_type, |code| {
                code.add_reg64(Register::Rax, Register::Rbx)
            }),
            Opcode::SubU64 => self.emit_binary_reg_op(node_index, node, result_type, |code| {
                code.sub_reg64(Register::Rax, Register::Rbx)
            }),
            Opcode::BoolAnd => self.emit_binary_reg_op(node_index, node, result_type, |code| {
                code.and_reg64(Register::Rax, Register::Rbx)
            }),
            Opcode::BoolOr => self.emit_binary_reg_op(node_index, node, result_type, |code| {
                code.or_reg64(Register::Rax, Register::Rbx)
            }),
            Opcode::BoolNot => {
                self.load_value_payload(Register::Rax, node.input0)?;
                self.code.test_reg64(Register::Rax, Register::Rax)?;
                self.code.setcc(ConditionCode::Equal, Register::R10)?;
                self.code.movzx_reg8(Register::Rax, Register::R10)?;
                self.store_value_from_register(node_index, result_type, Register::Rax)
            }
            Opcode::Select => self.emit_select(node_index, node, result_type),
            _ => Err(CodegenError::UnsupportedOpcode {
                opcode: node.opcode,
            }),
        }
    }

    fn emit_binary_reg_op(
        &mut self,
        node_index: usize,
        node: NodeRecord,
        result_type: PythType,
        op: impl FnOnce(&mut CodeBuffer) -> Result<()>,
    ) -> Result<()> {
        self.load_value_payload(Register::Rax, node.input0)?;
        self.load_value_payload(Register::Rbx, node.input1)?;
        op(&mut self.code)?;
        self.store_value_from_register(node_index, result_type, Register::Rax)
    }

    fn emit_select(
        &mut self,
        node_index: usize,
        node: NodeRecord,
        result_type: PythType,
    ) -> Result<()> {
        let false_label = self.next_local_label();
        let done_label = self.next_local_label();
        self.load_value_payload(Register::Rax, node.input0)?;
        self.code.test_reg64(Register::Rax, Register::Rax)?;
        self.emit_jcc(ConditionCode::Equal, false_label)?;
        self.load_value_payload(Register::Rax, node.input1)?;
        self.store_value_from_register(node_index, result_type, Register::Rax)?;
        self.emit_jmp(done_label)?;
        self.code.bind_label(false_label)?;
        self.load_value_payload(Register::Rax, node.input2)?;
        self.store_value_from_register(node_index, result_type, Register::Rax)?;
        self.code.bind_label(done_label)?;
        Ok(())
    }

    fn emit_jump(&mut self, node: NodeRecord) -> Result<()> {
        self.emit_block_parameter_moves(node.auxiliary0, node)?;
        let target = usize::try_from(node.auxiliary0).map_err(|_| CodegenError::AddressOverflow)?;
        self.emit_jmp(block_label(target))
    }

    fn emit_branch(&mut self, node: NodeRecord) -> Result<()> {
        let true_target =
            usize::try_from(node.auxiliary0).map_err(|_| CodegenError::AddressOverflow)?;
        let false_target =
            usize::try_from(node.auxiliary1).map_err(|_| CodegenError::AddressOverflow)?;
        self.load_value_payload(Register::Rax, node.input0)?;
        self.code.test_reg64(Register::Rax, Register::Rax)?;
        self.emit_jcc(ConditionCode::NotEqual, block_label(true_target))?;
        self.emit_jmp(block_label(false_target))
    }

    fn emit_block_parameter_moves(&mut self, target: u32, terminator: NodeRecord) -> Result<()> {
        let target = usize::try_from(target).map_err(|_| CodegenError::AddressOverflow)?;
        let block = self
            .package
            .blocks()
            .get(target)
            .ok_or(CodegenError::AddressOverflow)?;
        let count = usize::from(block.parameter_count);
        if count == 0 {
            return Ok(());
        }

        let first = usize::try_from(block.first_node).map_err(|_| CodegenError::AddressOverflow)?;
        let inputs = [
            terminator.input0,
            terminator.input1,
            terminator.input2,
            terminator.input3,
        ];
        let payload_registers = [Register::Rax, Register::Rbx, Register::Rcx, Register::Rdx];
        let meta_registers = [Register::R8, Register::R9, Register::R10, Register::R11];

        for index in 0..count {
            self.load_value_payload(payload_registers[index], inputs[index])?;
            self.load_value_meta(meta_registers[index], inputs[index])?;
        }
        for index in 0..count {
            let parameter_node = first
                .checked_add(index)
                .ok_or(CodegenError::AddressOverflow)?;
            self.store_value_raw(
                parameter_node,
                payload_registers[index],
                meta_registers[index],
            )?;
            self.metadata.block_parameter_moves += 1;
        }
        Ok(())
    }

    fn emit_return(&mut self, node_index: usize) -> Result<()> {
        self.store_typed_immediate(node_index, PythType::Unit, 0)?;
        self.write_exit_record(runtime_layout::GRAPH_EXIT_OK_STATUS, 0)?;
        self.emit_jmp(LABEL_DONE)
    }

    fn emit_exits(&mut self) -> Result<()> {
        for node_index in 0..self.package.nodes().len() {
            self.code.bind_label(node_budget_label(node_index)?)?;
            self.code.mov_imm64(Register::Rax, node_index as u64)?;
            self.store_scratch(self.layout.last_node_slot_offset(), Register::Rax)?;
            self.emit_jmp(LABEL_BUDGET_EXIT)?;
        }

        self.code.bind_label(LABEL_BUDGET_EXIT)?;
        self.write_exit_record(
            runtime_layout::GRAPH_EXIT_BUDGET_EXHAUSTED_STATUS,
            runtime_layout::RUNTIME_ERROR_BUDGET_EXHAUSTED,
        )?;
        self.emit_jmp(LABEL_DONE)?;

        self.code.bind_label(LABEL_RUNTIME_ERROR_EXIT)?;
        self.write_exit_record(
            runtime_layout::GRAPH_EXIT_RUNTIME_ERROR_STATUS,
            runtime_layout::RUNTIME_ERROR_UNSUPPORTED_OPCODE,
        )?;
        self.emit_jmp(LABEL_DONE)?;

        self.code.bind_label(LABEL_INVALID_BOOTSTRAP)?;
        self.code.ud2()?;

        self.code.bind_label(LABEL_DONE)?;
        self.code
            .mov_imm64(Register::Rax, self.layout.stack_frame_bytes() as u64)?;
        self.code.add_reg64(Register::Rsp, Register::Rax)?;
        self.code.pop(Register::R15)?;
        self.code.pop(Register::R14)?;
        self.code.pop(Register::R13)?;
        self.code.pop(Register::R12)?;
        self.code.ret()
    }

    fn write_exit_record(&mut self, status: u16, error_code: u16) -> Result<()> {
        self.load_scratch(Register::Rcx, self.layout.last_node_slot_offset())?;
        self.code.shl_reg64_imm8(Register::Rcx, 32)?;
        self.code.mov_imm64(
            Register::Rdx,
            u64::from(error_code) << 16 | u64::from(status),
        )?;
        self.code.or_reg64(Register::Rcx, Register::Rdx)?;
        self.code
            .mov_mem64_reg64(Memory::base(Register::R14), Register::Rcx)?;

        self.load_scratch(Register::Rax, self.layout.executed_slot_offset())?;
        self.code
            .mov_mem64_reg64(Memory::base_disp8(Register::R14, 8), Register::Rax)?;

        self.code.mov_imm64(
            Register::Rax,
            u64::from(runtime_layout::GRAPH_EXIT_RESULT_UNIT),
        )?;
        self.code
            .mov_mem64_reg64(Memory::base_disp8(Register::R14, 16), Register::Rax)?;
        self.code.mov_imm64(Register::Rax, 0)?;
        self.code
            .mov_mem64_reg64(Memory::base_disp8(Register::R14, 24), Register::Rax)?;

        self.code
            .mov_imm64(Register::Rax, runtime_layout::GRAPH_EXIT_SYSCALL)?;
        self.code.mov_reg64(Register::Rdi, Register::R14)?;
        self.code.mov_imm64(
            Register::Rsi,
            runtime_layout::GRAPH_EXIT_RECORD_BYTES as u64,
        )?;
        self.code.syscall()
    }

    fn store_typed_immediate(
        &mut self,
        node_index: usize,
        result_type: PythType,
        payload: u64,
    ) -> Result<()> {
        self.code.mov_imm64(Register::Rax, payload)?;
        self.store_value_from_register(node_index, result_type, Register::Rax)
    }

    fn store_value_from_register(
        &mut self,
        node_index: usize,
        result_type: PythType,
        payload: Register,
    ) -> Result<()> {
        let offset = self.value_offset_i32(node_index)?;
        self.code
            .mov_mem64_reg64(Memory::base_disp32(Register::R13, offset), payload)?;
        self.code
            .mov_imm64(Register::Rax, u64::from(result_type.code()))?;
        self.code.mov_mem64_reg64(
            Memory::base_disp32(Register::R13, offset + VALUE_SLOT_TYPE_OFFSET as i32),
            Register::Rax,
        )
    }

    fn store_value_raw(
        &mut self,
        node_index: usize,
        payload: Register,
        meta: Register,
    ) -> Result<()> {
        let offset = self.value_offset_i32(node_index)?;
        self.code
            .mov_mem64_reg64(Memory::base_disp32(Register::R13, offset), payload)?;
        self.code.mov_mem64_reg64(
            Memory::base_disp32(Register::R13, offset + VALUE_SLOT_TYPE_OFFSET as i32),
            meta,
        )
    }

    fn load_value_payload(&mut self, dst: Register, node_index: u32) -> Result<()> {
        if node_index == NO_VALUE {
            return Err(CodegenError::AddressOverflow);
        }
        let offset = self.value_offset_i32(
            usize::try_from(node_index).map_err(|_| CodegenError::AddressOverflow)?,
        )?;
        self.code
            .mov_reg64_mem64(dst, Memory::base_disp32(Register::R13, offset))
    }

    fn load_value_meta(&mut self, dst: Register, node_index: u32) -> Result<()> {
        if node_index == NO_VALUE {
            return Err(CodegenError::AddressOverflow);
        }
        let offset = self.value_offset_i32(
            usize::try_from(node_index).map_err(|_| CodegenError::AddressOverflow)?,
        )?;
        self.code.mov_reg64_mem64(
            dst,
            Memory::base_disp32(Register::R13, offset + VALUE_SLOT_TYPE_OFFSET as i32),
        )
    }

    fn load_scratch(&mut self, dst: Register, offset: usize) -> Result<()> {
        self.code
            .mov_reg64_mem64(dst, Memory::base_disp32(Register::R13, i32_offset(offset)?))
    }

    fn store_scratch(&mut self, offset: usize, src: Register) -> Result<()> {
        self.code
            .mov_mem64_reg64(Memory::base_disp32(Register::R13, i32_offset(offset)?), src)
    }

    fn increment_scratch(&mut self, offset: usize) -> Result<()> {
        self.load_scratch(Register::Rcx, offset)?;
        self.code.mov_imm64(Register::Rdx, 1)?;
        self.code.add_reg64(Register::Rcx, Register::Rdx)?;
        self.store_scratch(offset, Register::Rcx)
    }

    fn value_offset_i32(&self, node_index: usize) -> Result<i32> {
        i32_offset(self.layout.value_slot_offset(node_index)?)
    }

    fn emit_jmp(&mut self, label: Label) -> Result<()> {
        self.metadata.branch_patches += 1;
        self.code.jmp(label)
    }

    fn emit_jcc(&mut self, condition: ConditionCode, label: Label) -> Result<()> {
        self.metadata.branch_patches += 1;
        self.code.jcc(condition, label)
    }

    fn next_local_label(&mut self) -> Label {
        let label = Label::new(self.next_local_label);
        self.next_local_label += 1;
        label
    }
}

fn block_label(index: usize) -> Label {
    Label::new(LABEL_BLOCK_BASE + index as u32)
}

fn node_budget_label(index: usize) -> Result<Label> {
    let index = u32::try_from(index).map_err(|_| CodegenError::AddressOverflow)?;
    Ok(Label::new(LABEL_NODE_BUDGET_BASE + index))
}

fn i32_offset(offset: usize) -> Result<i32> {
    i32::try_from(offset).map_err(|_| CodegenError::AddressOverflow)
}
