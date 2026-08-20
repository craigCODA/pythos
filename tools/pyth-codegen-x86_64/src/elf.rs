use crate::{CodegenError, Result};

const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const SECTION_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_COUNT: usize = 3;
const SECTION_HEADER_COUNT: usize = 5;
const PAGE_SIZE: u64 = 0x1000;

const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 0x3E;
const EV_CURRENT: u32 = 1;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;
const SHT_PROGBITS: u32 = 1;
const SHT_STRTAB: u32 = 3;
const SHF_WRITE: u64 = 1;
const SHF_ALLOC: u64 = 2;
const SHF_EXECINSTR: u64 = 4;
const EMPTY_RODATA_SENTINEL: &[u8] = &[0];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElfImage {
    base_address: u64,
    text: Vec<u8>,
    rodata: Vec<u8>,
    data: Vec<u8>,
}

impl ElfImage {
    pub fn new(base_address: u64) -> Self {
        Self {
            base_address,
            text: Vec::new(),
            rodata: Vec::new(),
            data: Vec::new(),
        }
    }

    pub fn with_text(mut self, bytes: &[u8]) -> Self {
        self.text.clear();
        self.text.extend_from_slice(bytes);
        self
    }

    pub fn with_rodata(mut self, bytes: &[u8]) -> Self {
        self.rodata.clear();
        self.rodata.extend_from_slice(bytes);
        self
    }

    pub fn with_data(mut self, bytes: &[u8]) -> Self {
        self.data.clear();
        self.data.extend_from_slice(bytes);
        self
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        if !self.base_address.is_multiple_of(PAGE_SIZE) {
            return Err(CodegenError::InvalidBaseAddress {
                address: self.base_address,
            });
        }

        let rodata = if self.rodata.is_empty() {
            EMPTY_RODATA_SENTINEL
        } else {
            &self.rodata
        };

        let text_offset = PAGE_SIZE;
        let text_addr = self.base_address;
        let text_end = checked_add(text_offset, self.text.len() as u64)?;
        let rodata_offset = align_up(text_end, PAGE_SIZE)?;
        let rodata_addr = align_up(checked_add(text_addr, self.text.len() as u64)?, PAGE_SIZE)?;
        let rodata_end = checked_add(rodata_offset, rodata.len() as u64)?;
        let data_offset = align_up(rodata_end, PAGE_SIZE)?;
        let data_addr = align_up(checked_add(rodata_addr, rodata.len() as u64)?, PAGE_SIZE)?;
        let data_end = checked_add(data_offset, self.data.len() as u64)?;

        let shstrtab = SectionStringTable::new();
        let shstrtab_offset = align_up(data_end, 8)?;
        let shoff = align_up(
            checked_add(shstrtab_offset, shstrtab.bytes.len() as u64)?,
            8,
        )?;
        let file_len_u64 = checked_add(shoff, (SECTION_HEADER_COUNT * SECTION_HEADER_SIZE) as u64)?;
        let file_len = usize::try_from(file_len_u64).map_err(|_| CodegenError::AddressOverflow)?;

        let mut bytes = vec![0; file_len];
        write_elf_header(&mut bytes, text_addr, shoff)?;
        write_program_header(
            &mut bytes,
            0,
            ProgramHeader {
                flags: PF_R | PF_X,
                offset: text_offset,
                vaddr: text_addr,
                filesz: self.text.len() as u64,
                memsz: self.text.len() as u64,
            },
        )?;
        write_program_header(
            &mut bytes,
            1,
            ProgramHeader {
                flags: PF_R,
                offset: rodata_offset,
                vaddr: rodata_addr,
                filesz: rodata.len() as u64,
                memsz: rodata.len() as u64,
            },
        )?;
        write_program_header(
            &mut bytes,
            2,
            ProgramHeader {
                flags: PF_R | PF_W,
                offset: data_offset,
                vaddr: data_addr,
                filesz: self.data.len() as u64,
                memsz: self.data.len() as u64,
            },
        )?;

        write_range(&mut bytes, text_offset, &self.text)?;
        write_range(&mut bytes, rodata_offset, rodata)?;
        write_range(&mut bytes, data_offset, &self.data)?;
        write_range(&mut bytes, shstrtab_offset, &shstrtab.bytes)?;

        write_section_header(&mut bytes, shoff, 0, SectionHeader::null())?;
        write_section_header(
            &mut bytes,
            shoff,
            1,
            SectionHeader {
                name_offset: shstrtab.text_name,
                kind: SHT_PROGBITS,
                flags: SHF_ALLOC | SHF_EXECINSTR,
                addr: text_addr,
                offset: text_offset,
                size: self.text.len() as u64,
                addralign: 16,
            },
        )?;
        write_section_header(
            &mut bytes,
            shoff,
            2,
            SectionHeader {
                name_offset: shstrtab.rodata_name,
                kind: SHT_PROGBITS,
                flags: SHF_ALLOC,
                addr: rodata_addr,
                offset: rodata_offset,
                size: rodata.len() as u64,
                addralign: 1,
            },
        )?;
        write_section_header(
            &mut bytes,
            shoff,
            3,
            SectionHeader {
                name_offset: shstrtab.data_name,
                kind: SHT_PROGBITS,
                flags: SHF_ALLOC | SHF_WRITE,
                addr: data_addr,
                offset: data_offset,
                size: self.data.len() as u64,
                addralign: 8,
            },
        )?;
        write_section_header(
            &mut bytes,
            shoff,
            4,
            SectionHeader {
                name_offset: shstrtab.shstrtab_name,
                kind: SHT_STRTAB,
                flags: 0,
                addr: 0,
                offset: shstrtab_offset,
                size: shstrtab.bytes.len() as u64,
                addralign: 1,
            },
        )?;

        Ok(bytes)
    }
}

#[derive(Debug, Clone, Copy)]
struct ProgramHeader {
    flags: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
}

#[derive(Debug, Clone, Copy)]
struct SectionHeader {
    name_offset: u32,
    kind: u32,
    flags: u64,
    addr: u64,
    offset: u64,
    size: u64,
    addralign: u64,
}

impl SectionHeader {
    const fn null() -> Self {
        Self {
            name_offset: 0,
            kind: 0,
            flags: 0,
            addr: 0,
            offset: 0,
            size: 0,
            addralign: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct SectionStringTable {
    bytes: Vec<u8>,
    text_name: u32,
    rodata_name: u32,
    data_name: u32,
    shstrtab_name: u32,
}

impl SectionStringTable {
    fn new() -> Self {
        let mut bytes = vec![0];
        let text_name = append_name(&mut bytes, ".text");
        let rodata_name = append_name(&mut bytes, ".rodata");
        let data_name = append_name(&mut bytes, ".data");
        let shstrtab_name = append_name(&mut bytes, ".shstrtab");
        Self {
            bytes,
            text_name,
            rodata_name,
            data_name,
            shstrtab_name,
        }
    }
}

fn append_name(bytes: &mut Vec<u8>, name: &str) -> u32 {
    let offset = bytes.len() as u32;
    bytes.extend_from_slice(name.as_bytes());
    bytes.push(0);
    offset
}

fn write_elf_header(bytes: &mut [u8], entry: u64, shoff: u64) -> Result<()> {
    bytes[0..4].copy_from_slice(b"\x7FELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[6] = 1;
    write_u16(bytes, 16, ET_EXEC)?;
    write_u16(bytes, 18, EM_X86_64)?;
    write_u32(bytes, 20, EV_CURRENT)?;
    write_u64(bytes, 24, entry)?;
    write_u64(bytes, 32, ELF_HEADER_SIZE as u64)?;
    write_u64(bytes, 40, shoff)?;
    write_u32(bytes, 48, 0)?;
    write_u16(bytes, 52, ELF_HEADER_SIZE as u16)?;
    write_u16(bytes, 54, PROGRAM_HEADER_SIZE as u16)?;
    write_u16(bytes, 56, PROGRAM_HEADER_COUNT as u16)?;
    write_u16(bytes, 58, SECTION_HEADER_SIZE as u16)?;
    write_u16(bytes, 60, SECTION_HEADER_COUNT as u16)?;
    write_u16(bytes, 62, 4)?;
    Ok(())
}

fn write_program_header(bytes: &mut [u8], index: usize, header: ProgramHeader) -> Result<()> {
    let offset = checked_add_usize(ELF_HEADER_SIZE, index * PROGRAM_HEADER_SIZE)?;
    write_u32(bytes, offset, PT_LOAD)?;
    write_u32(bytes, offset + 4, header.flags)?;
    write_u64(bytes, offset + 8, header.offset)?;
    write_u64(bytes, offset + 16, header.vaddr)?;
    write_u64(bytes, offset + 24, header.vaddr)?;
    write_u64(bytes, offset + 32, header.filesz)?;
    write_u64(bytes, offset + 40, header.memsz)?;
    write_u64(bytes, offset + 48, PAGE_SIZE)?;
    Ok(())
}

fn write_section_header(
    bytes: &mut [u8],
    shoff: u64,
    index: usize,
    header: SectionHeader,
) -> Result<()> {
    let shoff = usize::try_from(shoff).map_err(|_| CodegenError::AddressOverflow)?;
    let offset = checked_add_usize(shoff, index * SECTION_HEADER_SIZE)?;
    write_u32(bytes, offset, header.name_offset)?;
    write_u32(bytes, offset + 4, header.kind)?;
    write_u64(bytes, offset + 8, header.flags)?;
    write_u64(bytes, offset + 16, header.addr)?;
    write_u64(bytes, offset + 24, header.offset)?;
    write_u64(bytes, offset + 32, header.size)?;
    write_u32(bytes, offset + 40, 0)?;
    write_u32(bytes, offset + 44, 0)?;
    write_u64(bytes, offset + 48, header.addralign)?;
    write_u64(bytes, offset + 56, 0)?;
    Ok(())
}

fn write_range(bytes: &mut [u8], offset: u64, data: &[u8]) -> Result<()> {
    let offset = usize::try_from(offset).map_err(|_| CodegenError::AddressOverflow)?;
    let end = checked_add_usize(offset, data.len())?;
    let len = bytes.len();
    let range = bytes
        .get_mut(offset..end)
        .ok_or(CodegenError::PatchOutOfBounds { offset, len })?;
    range.copy_from_slice(data);
    Ok(())
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<()> {
    write_range(bytes, offset as u64, &value.to_le_bytes())
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<()> {
    write_range(bytes, offset as u64, &value.to_le_bytes())
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<()> {
    write_range(bytes, offset as u64, &value.to_le_bytes())
}

fn align_up(value: u64, alignment: u64) -> Result<u64> {
    debug_assert!(alignment.is_power_of_two());
    let mask = alignment - 1;
    checked_add(value, mask).map(|value| value & !mask)
}

fn checked_add(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right).ok_or(CodegenError::AddressOverflow)
}

fn checked_add_usize(left: usize, right: usize) -> Result<usize> {
    left.checked_add(right).ok_or(CodegenError::AddressOverflow)
}
