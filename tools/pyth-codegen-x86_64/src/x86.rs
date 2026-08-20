use std::collections::BTreeMap;

use crate::{
    CodegenError, Result,
    patch::{Label, LabelPatch},
};

pub const DEFAULT_CODE_CAPACITY: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Register {
    Rax,
    Rcx,
    Rdx,
    Rbx,
    Rsp,
    Rbp,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

impl Register {
    const fn code(self) -> u8 {
        match self {
            Self::Rax => 0,
            Self::Rcx => 1,
            Self::Rdx => 2,
            Self::Rbx => 3,
            Self::Rsp => 4,
            Self::Rbp => 5,
            Self::Rsi => 6,
            Self::Rdi => 7,
            Self::R8 => 8,
            Self::R9 => 9,
            Self::R10 => 10,
            Self::R11 => 11,
            Self::R12 => 12,
            Self::R13 => 13,
            Self::R14 => 14,
            Self::R15 => 15,
        }
    }

    const fn low3(self) -> u8 {
        self.code() & 0b111
    }

    const fn rex_bit(self) -> bool {
        self.code() >= 8
    }

    const fn is_callee_saved(self) -> bool {
        matches!(
            self,
            Self::Rbx | Self::Rbp | Self::R12 | Self::R13 | Self::R14 | Self::R15
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConditionCode {
    Overflow = 0x0,
    NotOverflow = 0x1,
    Below = 0x2,
    AboveOrEqual = 0x3,
    Equal = 0x4,
    NotEqual = 0x5,
    BelowOrEqual = 0x6,
    Above = 0x7,
    Sign = 0x8,
    NotSign = 0x9,
    Parity = 0xA,
    NotParity = 0xB,
    Less = 0xC,
    GreaterOrEqual = 0xD,
    LessOrEqual = 0xE,
    Greater = 0xF,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    base: Register,
    displacement: i32,
    displacement_mode: DisplacementMode,
}

impl Memory {
    pub const fn base(base: Register) -> Self {
        Self {
            base,
            displacement: 0,
            displacement_mode: DisplacementMode::None,
        }
    }

    pub const fn base_disp8(base: Register, displacement: i8) -> Self {
        Self {
            base,
            displacement: displacement as i32,
            displacement_mode: DisplacementMode::Disp8,
        }
    }

    pub const fn base_disp32(base: Register, displacement: i32) -> Self {
        Self {
            base,
            displacement,
            displacement_mode: DisplacementMode::Disp32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplacementMode {
    None,
    Disp8,
    Disp32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingPatch {
    label: Label,
    patch: LabelPatch,
}

#[derive(Debug, Clone)]
pub struct CodeBuffer {
    bytes: Vec<u8>,
    capacity: usize,
    labels: BTreeMap<Label, usize>,
    patches: Vec<PendingPatch>,
}

impl Default for CodeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeBuffer {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CODE_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::new(),
            capacity,
            labels: BTreeMap::new(),
            patches: Vec::new(),
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn bind_label(&mut self, label: Label) -> Result<()> {
        if self.labels.contains_key(&label) {
            return Err(CodegenError::DuplicateLabel { label });
        }
        self.labels.insert(label, self.bytes.len());
        Ok(())
    }

    pub fn has_unresolved_label(&self, label: Label) -> bool {
        self.patches.iter().any(|patch| patch.label == label)
    }

    pub fn resolve_labels(&mut self) -> Result<()> {
        for pending in &self.patches {
            let target = *self
                .labels
                .get(&pending.label)
                .ok_or(CodegenError::UndefinedLabel {
                    label: pending.label,
                })?;
            let displacement = i64::try_from(target).map_err(|_| CodegenError::AddressOverflow)?
                - i64::try_from(pending.patch.origin_offset())
                    .map_err(|_| CodegenError::AddressOverflow)?;
            pending.patch.apply(&mut self.bytes, displacement)?;
        }
        self.patches.clear();
        Ok(())
    }

    pub fn patch_u64(&mut self, offset: usize, value: u64) -> Result<()> {
        let len = self.bytes.len();
        let range = self
            .bytes
            .get_mut(offset..offset + 8)
            .ok_or(CodegenError::PatchOutOfBounds { offset, len })?;
        range.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub fn mov_imm64(&mut self, dst: Register, immediate: u64) -> Result<()> {
        self.ensure_capacity(10)?;
        self.emit_rex(true, false, false, dst.rex_bit());
        self.bytes.push(0xB8 + dst.low3());
        self.bytes.extend_from_slice(&immediate.to_le_bytes());
        Ok(())
    }

    pub fn mov_reg64(&mut self, dst: Register, src: Register) -> Result<()> {
        self.emit_reg_reg(0x89, src, dst)
    }

    pub fn mov_mem64_reg64(&mut self, dst: Memory, src: Register) -> Result<()> {
        self.emit_reg_mem(0x89, src, dst)
    }

    pub fn mov_reg64_mem64(&mut self, dst: Register, src: Memory) -> Result<()> {
        self.emit_reg_mem(0x8B, dst, src)
    }

    pub fn lea(&mut self, dst: Register, src: Memory) -> Result<()> {
        self.emit_reg_mem(0x8D, dst, src)
    }

    pub fn add_reg64(&mut self, dst: Register, src: Register) -> Result<()> {
        self.emit_reg_reg(0x01, src, dst)
    }

    pub fn sub_reg64(&mut self, dst: Register, src: Register) -> Result<()> {
        self.emit_reg_reg(0x29, src, dst)
    }

    pub fn and_reg64(&mut self, dst: Register, src: Register) -> Result<()> {
        self.emit_reg_reg(0x21, src, dst)
    }

    pub fn or_reg64(&mut self, dst: Register, src: Register) -> Result<()> {
        self.emit_reg_reg(0x09, src, dst)
    }

    pub fn shl_reg64_imm8(&mut self, dst: Register, immediate: u8) -> Result<()> {
        self.ensure_capacity(4)?;
        self.emit_rex(true, false, false, dst.rex_bit());
        self.bytes.push(0xC1);
        self.bytes.push(modrm(0b11, 4, dst.low3()));
        self.bytes.push(immediate);
        Ok(())
    }

    pub fn xor_reg64(&mut self, dst: Register, src: Register) -> Result<()> {
        self.emit_reg_reg(0x31, src, dst)
    }

    pub fn cmp_reg64(&mut self, left: Register, right: Register) -> Result<()> {
        self.emit_reg_reg(0x39, right, left)
    }

    pub fn test_reg64(&mut self, left: Register, right: Register) -> Result<()> {
        self.emit_reg_reg(0x85, right, left)
    }

    pub fn push(&mut self, register: Register) -> Result<()> {
        self.validate_callee_saved("push", register)?;
        self.ensure_capacity(if register.rex_bit() { 2 } else { 1 })?;
        if register.rex_bit() {
            self.bytes.push(rex(false, false, false, true));
        }
        self.bytes.push(0x50 + register.low3());
        Ok(())
    }

    pub fn pop(&mut self, register: Register) -> Result<()> {
        self.validate_callee_saved("pop", register)?;
        self.ensure_capacity(if register.rex_bit() { 2 } else { 1 })?;
        if register.rex_bit() {
            self.bytes.push(rex(false, false, false, true));
        }
        self.bytes.push(0x58 + register.low3());
        Ok(())
    }

    pub fn setcc(&mut self, condition: ConditionCode, dst: Register) -> Result<()> {
        self.validate_byte_register("setcc", dst)?;
        let needs_rex = byte_register_needs_rex(dst);
        self.ensure_capacity(if needs_rex { 4 } else { 3 })?;
        if needs_rex {
            self.bytes.push(rex(false, false, false, dst.rex_bit()));
        }
        self.bytes.push(0x0F);
        self.bytes.push(0x90 + condition as u8);
        self.bytes.push(modrm(0b11, 0, dst.low3()));
        Ok(())
    }

    pub fn movzx_reg8(&mut self, dst: Register, src: Register) -> Result<()> {
        self.validate_byte_register("movzx", src)?;
        self.ensure_capacity(4)?;
        self.emit_rex(true, dst.rex_bit(), false, src.rex_bit());
        self.bytes.push(0x0F);
        self.bytes.push(0xB6);
        self.bytes.push(modrm(0b11, dst.low3(), src.low3()));
        Ok(())
    }

    pub fn call(&mut self, label: Label) -> Result<()> {
        self.emit_rel32_label(&[0xE8], label)
    }

    pub fn jmp(&mut self, label: Label) -> Result<()> {
        self.emit_rel32_label(&[0xE9], label)
    }

    pub fn jcc(&mut self, condition: ConditionCode, label: Label) -> Result<()> {
        self.emit_rel32_label(&[0x0F, 0x80 + condition as u8], label)
    }

    pub fn jz(&mut self, label: Label) -> Result<()> {
        self.jcc(ConditionCode::Equal, label)
    }

    pub fn syscall(&mut self) -> Result<()> {
        self.emit_slice(&[0x0F, 0x05])
    }

    pub fn ud2(&mut self) -> Result<()> {
        self.emit_slice(&[0x0F, 0x0B])
    }

    pub fn ret(&mut self) -> Result<()> {
        self.emit_slice(&[0xC3])
    }

    fn validate_callee_saved(&self, instruction: &'static str, register: Register) -> Result<()> {
        if register.is_callee_saved() {
            Ok(())
        } else {
            Err(CodegenError::InvalidRegister {
                instruction,
                register,
            })
        }
    }

    fn validate_byte_register(&self, instruction: &'static str, register: Register) -> Result<()> {
        if matches!(register, Register::Rsp) {
            Err(CodegenError::InvalidRegister {
                instruction,
                register,
            })
        } else {
            Ok(())
        }
    }

    fn emit_rel32_label(&mut self, opcode: &[u8], label: Label) -> Result<()> {
        let instruction_len = opcode
            .len()
            .checked_add(4)
            .ok_or(CodegenError::AddressOverflow)?;
        self.ensure_capacity(instruction_len)?;
        let displacement_offset = self
            .bytes
            .len()
            .checked_add(opcode.len())
            .ok_or(CodegenError::AddressOverflow)?;
        let origin_offset = self
            .bytes
            .len()
            .checked_add(instruction_len)
            .ok_or(CodegenError::AddressOverflow)?;
        self.bytes.extend_from_slice(opcode);
        self.bytes.extend_from_slice(&0_i32.to_le_bytes());
        self.patches.push(PendingPatch {
            label,
            patch: LabelPatch::new(displacement_offset, origin_offset),
        });
        Ok(())
    }

    fn emit_reg_reg(&mut self, opcode: u8, reg: Register, rm: Register) -> Result<()> {
        self.ensure_capacity(3)?;
        self.emit_rex(true, reg.rex_bit(), false, rm.rex_bit());
        self.bytes.push(opcode);
        self.bytes.push(modrm(0b11, reg.low3(), rm.low3()));
        Ok(())
    }

    fn emit_reg_mem(&mut self, opcode: u8, reg: Register, memory: Memory) -> Result<()> {
        let encoded = EncodedMemory::new(memory)?;
        self.ensure_capacity(3 + encoded.displacement_len)?;
        self.emit_rex(true, reg.rex_bit(), false, memory.base.rex_bit());
        self.bytes.push(opcode);
        self.bytes
            .push(modrm(encoded.mod_bits, reg.low3(), memory.base.low3()));
        match encoded.displacement_len {
            0 => {}
            1 => self.bytes.push(memory.displacement as i8 as u8),
            4 => self
                .bytes
                .extend_from_slice(&memory.displacement.to_le_bytes()),
            _ => unreachable!("encoded memory displacement is fixed-width"),
        }
        Ok(())
    }

    fn emit_rex(&mut self, w: bool, r: bool, x: bool, b: bool) {
        self.bytes.push(rex(w, r, x, b));
    }

    fn emit_slice(&mut self, bytes: &[u8]) -> Result<()> {
        self.ensure_capacity(bytes.len())?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn ensure_capacity(&self, additional: usize) -> Result<()> {
        let needed = self
            .bytes
            .len()
            .checked_add(additional)
            .ok_or(CodegenError::AddressOverflow)?;
        if needed > self.capacity {
            return Err(CodegenError::CapacityExceeded {
                needed,
                capacity: self.capacity,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EncodedMemory {
    mod_bits: u8,
    displacement_len: usize,
}

impl EncodedMemory {
    fn new(memory: Memory) -> Result<Self> {
        if matches!(memory.base, Register::Rsp | Register::R12) {
            return Err(CodegenError::InvalidMemoryBase {
                register: memory.base,
            });
        }
        let uses_rbp_encoding = matches!(memory.base, Register::Rbp | Register::R13);
        match memory.displacement_mode {
            DisplacementMode::None if !uses_rbp_encoding => Ok(Self {
                mod_bits: 0b00,
                displacement_len: 0,
            }),
            DisplacementMode::None | DisplacementMode::Disp8 => Ok(Self {
                mod_bits: 0b01,
                displacement_len: 1,
            }),
            DisplacementMode::Disp32 => Ok(Self {
                mod_bits: 0b10,
                displacement_len: 4,
            }),
        }
    }
}

const fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | ((w as u8) << 3) | ((r as u8) << 2) | ((x as u8) << 1) | (b as u8)
}

const fn byte_register_needs_rex(register: Register) -> bool {
    register.rex_bit()
        || matches!(
            register,
            Register::Rsp | Register::Rbp | Register::Rsi | Register::Rdi
        )
}

const fn modrm(mod_bits: u8, reg: u8, rm: u8) -> u8 {
    ((mod_bits & 0b11) << 6) | ((reg & 0b111) << 3) | (rm & 0b111)
}
