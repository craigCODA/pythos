use pyth_codegen_x86_64::{
    elf::ElfImage,
    patch::{Label, LabelPatch},
    x86::{CodeBuffer, ConditionCode, Memory, Register},
};

const ET_EXEC: u16 = 2;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;
const PT_LOAD: u32 = 1;
const SHT_PROGBITS: u32 = 1;
const SHT_STRTAB: u32 = 3;

#[test]
fn encodes_required_integer_branch_and_syscall_instructions() {
    let mut code = CodeBuffer::new();
    code.mov_imm64(Register::Rax, 0x1122_3344_5566_7788)
        .unwrap();
    code.mov_reg64(Register::Rbx, Register::Rax).unwrap();
    code.cmp_reg64(Register::Rax, Register::Rbx).unwrap();
    code.jcc(ConditionCode::Equal, Label::new(1)).unwrap();
    code.syscall().unwrap();

    assert_eq!(
        &code.bytes()[..10],
        &[0x48, 0xB8, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11]
    );
    assert_eq!(&code.bytes()[10..13], &[0x48, 0x89, 0xC3]);
    assert_eq!(&code.bytes()[13..16], &[0x48, 0x39, 0xD8]);
    assert_eq!(&code.bytes()[16..22], &[0x0F, 0x84, 0, 0, 0, 0]);
    assert_eq!(&code.bytes()[22..24], &[0x0F, 0x05]);
    assert!(code.has_unresolved_label(Label::new(1)));
}

#[test]
fn encodes_stack_memory_arithmetic_call_fault_and_return_subset() {
    let mut code = CodeBuffer::new();
    code.push(Register::R12).unwrap();
    code.push(Register::R15).unwrap();
    code.pop(Register::R15).unwrap();
    code.mov_mem64_reg64(Memory::base_disp8(Register::Rbp, -8), Register::Rax)
        .unwrap();
    code.mov_reg64_mem64(Register::R13, Memory::base_disp8(Register::Rbp, -8))
        .unwrap();
    code.lea(Register::R14, Memory::base_disp8(Register::Rbp, -16))
        .unwrap();
    code.add_reg64(Register::Rax, Register::Rbx).unwrap();
    code.sub_reg64(Register::Rax, Register::Rbx).unwrap();
    code.and_reg64(Register::Rax, Register::Rbx).unwrap();
    code.or_reg64(Register::Rax, Register::Rbx).unwrap();
    code.xor_reg64(Register::Rax, Register::Rbx).unwrap();
    code.test_reg64(Register::Rax, Register::Rbx).unwrap();
    code.setcc(ConditionCode::NotEqual, Register::R10).unwrap();
    code.movzx_reg8(Register::R11, Register::R10).unwrap();
    code.call(Label::new(7)).unwrap();
    code.jmp(Label::new(8)).unwrap();
    code.ud2().unwrap();
    code.ret().unwrap();

    assert!(
        code.bytes()
            .starts_with(&[0x41, 0x54, 0x41, 0x57, 0x41, 0x5F])
    );
    assert!(code.has_unresolved_label(Label::new(7)));
    assert!(code.has_unresolved_label(Label::new(8)));
    assert!(code.bytes().ends_with(&[0x0F, 0x0B, 0xC3]));
}

#[test]
fn patches_forward_and_backward_rel32_labels() {
    let mut code = CodeBuffer::new();
    let start = Label::new(1);
    let exit = Label::new(2);

    code.bind_label(start).unwrap();
    code.jmp(exit).unwrap();
    code.ret().unwrap();
    code.bind_label(exit).unwrap();
    code.call(start).unwrap();
    code.resolve_labels().unwrap();

    assert_eq!(&code.bytes()[0..5], &[0xE9, 1, 0, 0, 0]);
    assert_eq!(&code.bytes()[6..11], &[0xE8, 0xF5, 0xFF, 0xFF, 0xFF]);
    assert!(!code.has_unresolved_label(start));
    assert!(!code.has_unresolved_label(exit));
}

#[test]
fn patcher_rejects_rel32_displacement_overflow() {
    let mut bytes = vec![0; 4];
    let patch = LabelPatch::new(0, 5);
    let err = patch
        .apply(&mut bytes, i64::from(i32::MAX) + 1)
        .unwrap_err();
    assert!(format!("{err:?}").contains("RelativeDisplacementOutOfRange"));
}

#[test]
fn encoder_rejects_invalid_operands_and_capacity_overflow() {
    let mut code = CodeBuffer::with_capacity(2);
    assert!(code.mov_imm64(Register::Rax, 0).is_err());

    let mut code = CodeBuffer::new();
    assert!(code.push(Register::Rax).is_err());
    assert!(code.pop(Register::R11).is_err());
    assert!(code.setcc(ConditionCode::Equal, Register::Rsp).is_err());
    assert!(
        code.mov_mem64_reg64(Memory::base(Register::Rsp), Register::Rax)
            .is_err()
    );
}

#[test]
fn elf_writer_produces_exec_with_rx_text_r_rodata_and_rw_data() {
    let elf = ElfImage::new(0x0040_0000)
        .with_text(&[0xC3])
        .with_rodata(b"hello")
        .with_data(&[0u8; 32])
        .encode()
        .unwrap();
    let parsed = ParsedElf::parse(&elf).unwrap();

    assert_eq!(parsed.file_type, ET_EXEC);
    assert_eq!(parsed.machine, 0x3E);
    assert_eq!(parsed.entry, 0x0040_0000);
    assert!(parsed.has_load_flags(PF_R | PF_X));
    assert!(parsed.has_load_flags(PF_R));
    assert!(parsed.has_load_flags(PF_R | PF_W));
    assert!(!parsed.has_load_flags(PF_R | PF_W | PF_X));
    assert_eq!(parsed.section_addr(".text"), Some(0x0040_0000));
    assert_eq!(parsed.section_kind(".text"), Some(SHT_PROGBITS));
    assert_eq!(parsed.section_kind(".rodata"), Some(SHT_PROGBITS));
    assert_eq!(parsed.section_kind(".data"), Some(SHT_PROGBITS));
    assert_eq!(parsed.section_kind(".shstrtab"), Some(SHT_STRTAB));
    assert_eq!(parsed.section_flags(".text"), Some(0x6));
    assert_eq!(parsed.section_flags(".rodata"), Some(0x2));
    assert_eq!(parsed.section_flags(".data"), Some(0x3));
}

#[test]
fn elf_writer_aligns_load_segments_to_pages() {
    let elf = ElfImage::new(0x0040_0000)
        .with_text(&[0x90; 17])
        .with_rodata(&[0xAB; 9])
        .with_data(&[0xCD; 33])
        .encode()
        .unwrap();
    let parsed = ParsedElf::parse(&elf).unwrap();

    assert_eq!(parsed.load_address(PF_R | PF_X), Some(0x0040_0000));
    assert_eq!(parsed.load_address(PF_R), Some(0x0040_1000));
    assert_eq!(parsed.load_address(PF_R | PF_W), Some(0x0040_2000));
    assert_eq!(parsed.load_file_alignment(PF_R | PF_X), Some(0x1000));
    assert_eq!(parsed.load_file_alignment(PF_R), Some(0x1000));
    assert_eq!(parsed.load_file_alignment(PF_R | PF_W), Some(0x1000));
}

#[test]
fn elf_writer_materializes_empty_rodata_as_valid_load_segment() {
    let elf = ElfImage::new(0x0040_0000)
        .with_text(&[0xC3])
        .with_rodata(&[])
        .with_data(&[0xCD; 33])
        .encode()
        .unwrap();
    let parsed = ParsedElf::parse(&elf).unwrap();

    assert_eq!(parsed.load_address(PF_R | PF_X), Some(0x0040_0000));
    assert_eq!(parsed.load_address(PF_R), Some(0x0040_1000));
    assert_eq!(parsed.load_address(PF_R | PF_W), Some(0x0040_2000));
    assert_eq!(parsed.load_file_size(PF_R), Some(1));
    assert_eq!(parsed.section_size(".rodata"), Some(1));
}

#[derive(Debug)]
struct ParsedElf<'a> {
    bytes: &'a [u8],
    file_type: u16,
    machine: u16,
    entry: u64,
    program_headers: Vec<ProgramHeader>,
    section_headers: Vec<SectionHeader>,
    section_names: Vec<String>,
}

#[derive(Debug)]
struct ProgramHeader {
    kind: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
}

#[derive(Debug)]
struct SectionHeader {
    name_offset: u32,
    kind: u32,
    flags: u64,
    addr: u64,
    offset: u64,
    size: u64,
}

impl<'a> ParsedElf<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < 64 {
            return Err("ELF header too short".to_string());
        }
        if &bytes[0..4] != b"\x7FELF" {
            return Err("bad ELF magic".to_string());
        }
        if bytes[4] != 2 || bytes[5] != 1 {
            return Err("not ELF64 little-endian".to_string());
        }

        let file_type = read_u16(bytes, 16)?;
        let machine = read_u16(bytes, 18)?;
        let entry = read_u64(bytes, 24)?;
        let phoff = read_u64(bytes, 32)? as usize;
        let shoff = read_u64(bytes, 40)? as usize;
        let ehsize = read_u16(bytes, 52)?;
        let phentsize = read_u16(bytes, 54)? as usize;
        let phnum = read_u16(bytes, 56)? as usize;
        let shentsize = read_u16(bytes, 58)? as usize;
        let shnum = read_u16(bytes, 60)? as usize;
        let shstrndx = read_u16(bytes, 62)? as usize;

        if ehsize != 64 {
            return Err("unexpected ELF header size".to_string());
        }
        if phentsize != 56 {
            return Err("unexpected program header size".to_string());
        }
        if shentsize != 64 {
            return Err("unexpected section header size".to_string());
        }

        let mut program_headers = Vec::new();
        for index in 0..phnum {
            let off = phoff + index * phentsize;
            program_headers.push(ProgramHeader {
                kind: read_u32(bytes, off)?,
                flags: read_u32(bytes, off + 4)?,
                offset: read_u64(bytes, off + 8)?,
                vaddr: read_u64(bytes, off + 16)?,
                filesz: read_u64(bytes, off + 32)?,
                memsz: read_u64(bytes, off + 40)?,
                align: read_u64(bytes, off + 48)?,
            });
        }

        let mut section_headers = Vec::new();
        for index in 0..shnum {
            let off = shoff + index * shentsize;
            section_headers.push(SectionHeader {
                name_offset: read_u32(bytes, off)?,
                kind: read_u32(bytes, off + 4)?,
                flags: read_u64(bytes, off + 8)?,
                addr: read_u64(bytes, off + 16)?,
                offset: read_u64(bytes, off + 24)?,
                size: read_u64(bytes, off + 32)?,
            });
        }

        if shstrndx >= section_headers.len() {
            return Err("bad shstrndx".to_string());
        }
        let shstr = &section_headers[shstrndx];
        let shstr_start = shstr.offset as usize;
        let shstr_end = shstr_start + shstr.size as usize;
        let shstr_bytes = bytes
            .get(shstr_start..shstr_end)
            .ok_or_else(|| "bad shstrtab range".to_string())?;
        let section_names = section_headers
            .iter()
            .map(|header| read_cstr(shstr_bytes, header.name_offset as usize))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            bytes,
            file_type,
            machine,
            entry,
            program_headers,
            section_headers,
            section_names,
        })
    }

    fn has_load_flags(&self, flags: u32) -> bool {
        self.program_headers
            .iter()
            .any(|header| header.kind == PT_LOAD && header.flags == flags)
    }

    fn load_address(&self, flags: u32) -> Option<u64> {
        self.program_headers
            .iter()
            .find(|header| header.kind == PT_LOAD && header.flags == flags)
            .map(|header| header.vaddr)
    }

    fn load_file_alignment(&self, flags: u32) -> Option<u64> {
        self.program_headers
            .iter()
            .find(|header| header.kind == PT_LOAD && header.flags == flags)
            .map(|header| {
                assert_eq!(header.offset % 0x1000, 0);
                assert_eq!(header.vaddr % 0x1000, 0);
                assert_eq!(header.filesz, header.memsz);
                assert!(header.offset + header.filesz <= self.bytes.len() as u64);
                header.align
            })
    }

    fn load_file_size(&self, flags: u32) -> Option<u64> {
        self.program_headers
            .iter()
            .find(|header| header.kind == PT_LOAD && header.flags == flags)
            .map(|header| {
                assert_eq!(header.filesz, header.memsz);
                assert!(header.filesz > 0);
                header.filesz
            })
    }

    fn section_addr(&self, name: &str) -> Option<u64> {
        self.section_header(name).map(|header| header.addr)
    }

    fn section_size(&self, name: &str) -> Option<u64> {
        self.section_header(name).map(|header| header.size)
    }

    fn section_flags(&self, name: &str) -> Option<u64> {
        self.section_header(name).map(|header| header.flags)
    }

    fn section_kind(&self, name: &str) -> Option<u32> {
        self.section_header(name).map(|header| header.kind)
    }

    fn section_header(&self, name: &str) -> Option<&SectionHeader> {
        self.section_headers
            .iter()
            .enumerate()
            .find(|(index, _)| self.section_names[*index] == name)
            .map(|(_, header)| {
                assert!(header.offset + header.size <= self.bytes.len() as u64);
                header
            })
    }
}

fn read_cstr(bytes: &[u8], offset: usize) -> Result<String, String> {
    if offset == 0 {
        return Ok(String::new());
    }
    let tail = bytes
        .get(offset..)
        .ok_or_else(|| "bad string offset".to_string())?;
    let len = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| "unterminated string".to_string())?;
    core::str::from_utf8(&tail[..len])
        .map(|name| name.to_string())
        .map_err(|_| "non-utf8 section name".to_string())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("u16 out of bounds at {offset}"))?;
    Ok(u16::from_le_bytes(value.try_into().unwrap()))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("u32 out of bounds at {offset}"))?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| format!("u64 out of bounds at {offset}"))?;
    Ok(u64::from_le_bytes(value.try_into().unwrap()))
}
