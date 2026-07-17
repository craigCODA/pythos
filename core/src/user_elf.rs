#![cfg_attr(test, allow(dead_code))]

use crate::memory::physical::PAGE_SIZE;
use core::convert::TryFrom;

const EI_CLASS_64: u8 = 2;
const EI_DATA_LITTLE_ENDIAN: u8 = 1;
const ELF_VERSION_CURRENT: u8 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 0x3E;
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const PF_X: u32 = 0x1;
const PF_W: u32 = 0x2;
const PF_R: u32 = 0x4;
const ELF64_EHDR_SIZE: usize = 64;
const ELF64_PHDR_SIZE: u16 = 56;
const MAX_LOAD_SEGMENTS: usize = 8;
const USER_VIRT_MIN: u64 = 0x0020_0000;
const USER_VIRT_MAX: u64 = 0x0000_8000_0000_0000;
const KERNEL_VIRT_MIN: u64 = 0xFFFF_FFFF_8000_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserElfError {
    TooShort,
    BadMagic,
    UnsupportedClass,
    UnsupportedEndian,
    UnsupportedVersion,
    UnsupportedType,
    UnsupportedMachine,
    BadProgramHeaderSize,
    BadProgramHeaderTable,
    UnsupportedSegment,
    EmptySegment,
    BufferRange,
    RangeOverflow,
    KernelRange,
    BadAlignment,
    WxSegment,
    TooManySegments,
    OverlappingSegments,
    EntryOutsideExecutable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadSegment {
    file_offset: u64,
    virtual_start: u64,
    file_size: u64,
    mem_size: u64,
    flags: u32,
    page_start: u64,
    page_end: u64,
}

impl LoadSegment {
    pub const fn file_offset(&self) -> u64 {
        self.file_offset
    }

    pub const fn virtual_start(&self) -> u64 {
        self.virtual_start
    }

    pub const fn file_size(&self) -> u64 {
        self.file_size
    }

    pub const fn page_start(&self) -> u64 {
        self.page_start
    }

    pub const fn page_len(&self) -> u64 {
        self.page_end - self.page_start
    }

    pub const fn writable(&self) -> bool {
        self.flags & PF_W != 0
    }

    pub const fn executable(&self) -> bool {
        self.flags & PF_X != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserElfImage {
    entry: u64,
    segments: [Option<LoadSegment>; MAX_LOAD_SEGMENTS],
    segment_count: usize,
}

impl UserElfImage {
    pub const fn entry(&self) -> u64 {
        self.entry
    }

    pub const fn segment_count(&self) -> usize {
        self.segment_count
    }

    pub fn segment(&self, index: usize) -> Option<&LoadSegment> {
        if index >= self.segment_count {
            return None;
        }
        self.segments[index].as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserElfRejectionProof {
    pub buffer_range_denied: bool,
    pub wx_segment_denied: bool,
    pub kernel_range_denied: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadedUserElf {
    entry: u64,
    segment_count: usize,
    bss_zeroed: bool,
}

impl LoadedUserElf {
    pub const fn new(entry: u64, segment_count: usize, bss_zeroed: bool) -> Self {
        Self {
            entry,
            segment_count,
            bss_zeroed,
        }
    }

    pub const fn entry(&self) -> u64 {
        self.entry
    }

    pub const fn segment_count(&self) -> usize {
        self.segment_count
    }

    pub const fn bss_zeroed(&self) -> bool {
        self.bss_zeroed
    }
}

pub fn validate(bytes: &[u8]) -> Result<UserElfImage, UserElfError> {
    if bytes.len() < ELF64_EHDR_SIZE {
        return Err(UserElfError::TooShort);
    }
    if &bytes[0..4] != b"\x7fELF" {
        return Err(UserElfError::BadMagic);
    }
    if bytes[4] != EI_CLASS_64 {
        return Err(UserElfError::UnsupportedClass);
    }
    if bytes[5] != EI_DATA_LITTLE_ENDIAN {
        return Err(UserElfError::UnsupportedEndian);
    }
    if bytes[6] != ELF_VERSION_CURRENT || read_u32(bytes, 20)? != u32::from(ELF_VERSION_CURRENT) {
        return Err(UserElfError::UnsupportedVersion);
    }
    if read_u16(bytes, 16)? != ET_EXEC {
        return Err(UserElfError::UnsupportedType);
    }
    if read_u16(bytes, 18)? != EM_X86_64 {
        return Err(UserElfError::UnsupportedMachine);
    }

    let entry = read_u64(bytes, 24)?;
    let phoff = read_u64(bytes, 32)?;
    let phentsize = read_u16(bytes, 54)?;
    let phnum = usize::from(read_u16(bytes, 56)?);
    if phentsize != ELF64_PHDR_SIZE {
        return Err(UserElfError::BadProgramHeaderSize);
    }
    if phnum == 0 {
        return Err(UserElfError::BadProgramHeaderTable);
    }
    let phoff_usize = usize::try_from(phoff).map_err(|_| UserElfError::BadProgramHeaderTable)?;
    let ph_table_len = phnum
        .checked_mul(usize::from(ELF64_PHDR_SIZE))
        .ok_or(UserElfError::RangeOverflow)?;
    let ph_end = phoff_usize
        .checked_add(ph_table_len)
        .ok_or(UserElfError::RangeOverflow)?;
    if ph_end > bytes.len() {
        return Err(UserElfError::BadProgramHeaderTable);
    }

    let mut image = UserElfImage {
        entry,
        segments: [None; MAX_LOAD_SEGMENTS],
        segment_count: 0,
    };
    let mut ph_index = 0;
    while ph_index < phnum {
        let phdr = phoff_usize
            .checked_add(
                ph_index
                    .checked_mul(usize::from(ELF64_PHDR_SIZE))
                    .ok_or(UserElfError::RangeOverflow)?,
            )
            .ok_or(UserElfError::RangeOverflow)?;
        parse_program_header(bytes, phdr, &mut image)?;
        ph_index += 1;
    }

    if image.segment_count == 0 {
        return Err(UserElfError::BadProgramHeaderTable);
    }
    if !entry_inside_executable_segment(entry, &image) {
        return Err(UserElfError::EntryOutsideExecutable);
    }

    Ok(image)
}

#[cfg(test)]
pub fn copy_segment_to_buffer(
    elf: &[u8],
    segment: &LoadSegment,
    destination: &mut [u8],
) -> Result<(), UserElfError> {
    let page_len = usize::try_from(segment.page_len()).map_err(|_| UserElfError::RangeOverflow)?;
    if destination.len() < page_len {
        return Err(UserElfError::BufferRange);
    }
    let file_offset =
        usize::try_from(segment.file_offset).map_err(|_| UserElfError::BufferRange)?;
    let file_size = usize::try_from(segment.file_size).map_err(|_| UserElfError::BufferRange)?;
    let file_end = file_offset
        .checked_add(file_size)
        .ok_or(UserElfError::BufferRange)?;
    let source = elf
        .get(file_offset..file_end)
        .ok_or(UserElfError::BufferRange)?;
    let destination_offset = usize::try_from(segment.virtual_start - segment.page_start)
        .map_err(|_| UserElfError::RangeOverflow)?;
    let destination_end = destination_offset
        .checked_add(file_size)
        .ok_or(UserElfError::BufferRange)?;
    if destination_end > destination.len() {
        return Err(UserElfError::BufferRange);
    }

    destination[..page_len].fill(0);
    destination[destination_offset..destination_end].copy_from_slice(source);
    Ok(())
}

pub fn run_rejection_self_tests() -> Result<UserElfRejectionProof, UserElfError> {
    let buffer_range_denied =
        validate(&rejection_elf(RejectCase::BufferRange)) == Err(UserElfError::BufferRange);
    let wx_segment_denied =
        validate(&rejection_elf(RejectCase::WxSegment)) == Err(UserElfError::WxSegment);
    let kernel_range_denied =
        validate(&rejection_elf(RejectCase::KernelRange)) == Err(UserElfError::KernelRange);
    if !buffer_range_denied {
        return Err(UserElfError::BufferRange);
    }
    if !wx_segment_denied {
        return Err(UserElfError::WxSegment);
    }
    if !kernel_range_denied {
        return Err(UserElfError::KernelRange);
    }
    Ok(UserElfRejectionProof {
        buffer_range_denied,
        wx_segment_denied,
        kernel_range_denied,
    })
}

fn parse_program_header(
    bytes: &[u8],
    phdr: usize,
    image: &mut UserElfImage,
) -> Result<(), UserElfError> {
    let p_type = read_u32(bytes, phdr)?;
    if p_type != PT_LOAD {
        let _explicit_dynamic_reject = p_type == PT_INTERP || p_type == PT_DYNAMIC;
        return Err(UserElfError::UnsupportedSegment);
    }
    if image.segment_count >= MAX_LOAD_SEGMENTS {
        return Err(UserElfError::TooManySegments);
    }

    let flags = read_u32(bytes, phdr + 4)?;
    let offset = read_u64(bytes, phdr + 8)?;
    let vaddr = read_u64(bytes, phdr + 16)?;
    let filesz = read_u64(bytes, phdr + 32)?;
    let memsz = read_u64(bytes, phdr + 40)?;
    let align = read_u64(bytes, phdr + 48)?;

    if memsz == 0 || filesz > memsz {
        return Err(UserElfError::EmptySegment);
    }
    let file_end = offset
        .checked_add(filesz)
        .ok_or(UserElfError::BufferRange)?;
    if file_end > bytes.len() as u64 {
        return Err(UserElfError::BufferRange);
    }
    let virtual_end = vaddr
        .checked_add(memsz)
        .ok_or(UserElfError::RangeOverflow)?;
    if vaddr < USER_VIRT_MIN
        || virtual_end > USER_VIRT_MAX
        || vaddr >= KERNEL_VIRT_MIN
        || virtual_end >= KERNEL_VIRT_MIN
    {
        return Err(UserElfError::KernelRange);
    }
    if align == 0 || !align.is_power_of_two() || vaddr % align != offset % align {
        return Err(UserElfError::BadAlignment);
    }
    if flags & PF_W != 0 && flags & PF_X != 0 {
        return Err(UserElfError::WxSegment);
    }

    let page_start = align_down(vaddr);
    let page_end = align_up(virtual_end)?;
    let mut existing = 0;
    while existing < image.segment_count {
        let segment = image.segments[existing].ok_or(UserElfError::BadProgramHeaderTable)?;
        if page_start < segment.page_end && segment.page_start < page_end {
            return Err(UserElfError::OverlappingSegments);
        }
        existing += 1;
    }

    image.segments[image.segment_count] = Some(LoadSegment {
        file_offset: offset,
        virtual_start: vaddr,
        file_size: filesz,
        mem_size: memsz,
        flags,
        page_start,
        page_end,
    });
    image.segment_count += 1;
    Ok(())
}

fn entry_inside_executable_segment(entry: u64, image: &UserElfImage) -> bool {
    let mut index = 0;
    while index < image.segment_count {
        if let Some(segment) = image.segments[index]
            && segment.executable()
            && entry >= segment.virtual_start
            && entry < segment.virtual_start.saturating_add(segment.mem_size)
        {
            return true;
        }
        index += 1;
    }
    false
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, UserElfError> {
    let end = offset.checked_add(2).ok_or(UserElfError::RangeOverflow)?;
    let raw = bytes.get(offset..end).ok_or(UserElfError::TooShort)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, UserElfError> {
    let end = offset.checked_add(4).ok_or(UserElfError::RangeOverflow)?;
    let raw = bytes.get(offset..end).ok_or(UserElfError::TooShort)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, UserElfError> {
    let end = offset.checked_add(8).ok_or(UserElfError::RangeOverflow)?;
    let raw = bytes.get(offset..end).ok_or(UserElfError::TooShort)?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}

fn align_down(value: u64) -> u64 {
    value & !(PAGE_SIZE - 1)
}

fn align_up(value: u64) -> Result<u64, UserElfError> {
    value
        .checked_add(PAGE_SIZE - 1)
        .map(align_down)
        .ok_or(UserElfError::RangeOverflow)
}

#[derive(Clone, Copy)]
enum RejectCase {
    BufferRange,
    WxSegment,
    KernelRange,
}

fn rejection_elf(case: RejectCase) -> [u8; 128] {
    let mut elf = [0u8; 128];
    write_elf_header(&mut elf, 1, 0x0040_0000);
    match case {
        RejectCase::BufferRange => write_phdr(
            &mut elf,
            0,
            PhdrSpec::new(PT_LOAD, PF_R, 120, 0x0040_0000, 16, 16),
        ),
        RejectCase::WxSegment => write_phdr(
            &mut elf,
            0,
            PhdrSpec::new(PT_LOAD, PF_R | PF_W | PF_X, 0, 0x0040_0000, 0, 16),
        ),
        RejectCase::KernelRange => write_phdr(
            &mut elf,
            0,
            PhdrSpec::new(PT_LOAD, PF_R, 0, KERNEL_VIRT_MIN, 0, 16),
        ),
    }
    elf
}

fn write_elf_header(elf: &mut [u8], phnum: u16, entry: u64) {
    elf[0..4].copy_from_slice(b"\x7fELF");
    elf[4] = EI_CLASS_64;
    elf[5] = EI_DATA_LITTLE_ENDIAN;
    elf[6] = ELF_VERSION_CURRENT;
    elf[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
    elf[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
    elf[20..24].copy_from_slice(&u32::from(ELF_VERSION_CURRENT).to_le_bytes());
    elf[24..32].copy_from_slice(&entry.to_le_bytes());
    elf[32..40].copy_from_slice(&(ELF64_EHDR_SIZE as u64).to_le_bytes());
    elf[52..54].copy_from_slice(&(ELF64_EHDR_SIZE as u16).to_le_bytes());
    elf[54..56].copy_from_slice(&ELF64_PHDR_SIZE.to_le_bytes());
    elf[56..58].copy_from_slice(&phnum.to_le_bytes());
}

#[derive(Clone, Copy)]
struct PhdrSpec {
    p_type: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
}

impl PhdrSpec {
    const fn new(
        p_type: u32,
        flags: u32,
        offset: u64,
        vaddr: u64,
        filesz: u64,
        memsz: u64,
    ) -> Self {
        Self {
            p_type,
            flags,
            offset,
            vaddr,
            filesz,
            memsz,
        }
    }
}

fn write_phdr(elf: &mut [u8], index: usize, spec: PhdrSpec) {
    let entry = ELF64_EHDR_SIZE + index * usize::from(ELF64_PHDR_SIZE);
    elf[entry..entry + 4].copy_from_slice(&spec.p_type.to_le_bytes());
    elf[entry + 4..entry + 8].copy_from_slice(&spec.flags.to_le_bytes());
    elf[entry + 8..entry + 16].copy_from_slice(&spec.offset.to_le_bytes());
    elf[entry + 16..entry + 24].copy_from_slice(&spec.vaddr.to_le_bytes());
    elf[entry + 24..entry + 32].copy_from_slice(&spec.vaddr.to_le_bytes());
    elf[entry + 32..entry + 40].copy_from_slice(&spec.filesz.to_le_bytes());
    elf[entry + 40..entry + 48].copy_from_slice(&spec.memsz.to_le_bytes());
    elf[entry + 48..entry + 56].copy_from_slice(&PAGE_SIZE.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use std::vec;
    use std::vec::Vec;

    use super::*;

    fn minimal_user_elf() -> Vec<u8> {
        let text = b"\x90\xc3";
        let data = b"DATA";
        let text_offset = 0x1000usize;
        let data_offset = 0x2000usize;
        let mut elf = vec![0u8; data_offset + data.len()];
        write_elf_header(&mut elf, 2, 0x0040_0000);
        write_phdr(
            &mut elf,
            0,
            PhdrSpec::new(
                PT_LOAD,
                PF_R | PF_X,
                text_offset as u64,
                0x0040_0000,
                text.len() as u64,
                text.len() as u64,
            ),
        );
        write_phdr(
            &mut elf,
            1,
            PhdrSpec::new(
                PT_LOAD,
                PF_R | PF_W,
                data_offset as u64,
                0x0040_1000,
                data.len() as u64,
                16,
            ),
        );
        elf[text_offset..text_offset + text.len()].copy_from_slice(text);
        elf[data_offset..data_offset + data.len()].copy_from_slice(data);
        elf
    }

    fn elf_with_bad_file_range() -> Vec<u8> {
        let mut elf = minimal_user_elf();
        let entry = ELF64_EHDR_SIZE;
        let bad_offset = elf.len() as u64 - 1;
        elf[entry + 8..entry + 16].copy_from_slice(&bad_offset.to_le_bytes());
        elf[entry + 32..entry + 40].copy_from_slice(&16u64.to_le_bytes());
        elf[entry + 40..entry + 48].copy_from_slice(&16u64.to_le_bytes());
        elf
    }

    fn elf_with_wx_segment() -> Vec<u8> {
        let mut elf = minimal_user_elf();
        let entry = ELF64_EHDR_SIZE;
        elf[entry + 4..entry + 8].copy_from_slice(&(PF_R | PF_W | PF_X).to_le_bytes());
        elf
    }

    fn elf_with_kernel_range() -> Vec<u8> {
        let mut elf = minimal_user_elf();
        let entry = ELF64_EHDR_SIZE;
        elf[entry + 16..entry + 24].copy_from_slice(&KERNEL_VIRT_MIN.to_le_bytes());
        elf
    }

    fn elf_with_overlapping_segments() -> Vec<u8> {
        let mut elf = minimal_user_elf();
        let second = ELF64_EHDR_SIZE + usize::from(ELF64_PHDR_SIZE);
        elf[second + 8..second + 16].copy_from_slice(&0x1001u64.to_le_bytes());
        elf[second + 16..second + 24].copy_from_slice(&0x0040_0001u64.to_le_bytes());
        elf
    }

    fn elf_with_interp_segment() -> Vec<u8> {
        let mut elf = minimal_user_elf();
        let entry = ELF64_EHDR_SIZE;
        elf[entry..entry + 4].copy_from_slice(&PT_INTERP.to_le_bytes());
        elf
    }

    fn elf_with_dynamic_segment() -> Vec<u8> {
        let mut elf = minimal_user_elf();
        let entry = ELF64_EHDR_SIZE;
        elf[entry..entry + 4].copy_from_slice(&PT_DYNAMIC.to_le_bytes());
        elf
    }

    fn copy_segment_for_test(
        elf: &[u8],
        segment: &LoadSegment,
        destination: &mut [u8],
    ) -> Result<(), UserElfError> {
        copy_segment_to_buffer(elf, segment, destination)
    }

    #[test]
    fn minimal_user_elf_validates() {
        let image = validate(&minimal_user_elf()).unwrap();

        assert_eq!(image.entry(), 0x00400000);
        assert_eq!(image.segment_count(), 2);
    }

    #[test]
    fn segment_exceeding_buffer_is_rejected() {
        assert_eq!(
            validate(&elf_with_bad_file_range()),
            Err(UserElfError::BufferRange)
        );
    }

    #[test]
    fn writable_executable_segment_is_rejected() {
        assert_eq!(
            validate(&elf_with_wx_segment()),
            Err(UserElfError::WxSegment)
        );
    }

    #[test]
    fn kernel_higher_half_segment_is_rejected() {
        assert_eq!(
            validate(&elf_with_kernel_range()),
            Err(UserElfError::KernelRange)
        );
    }

    #[test]
    fn overlapping_page_ranges_are_rejected() {
        assert_eq!(
            validate(&elf_with_overlapping_segments()),
            Err(UserElfError::OverlappingSegments)
        );
    }

    #[test]
    fn interp_segment_is_rejected() {
        assert_eq!(
            validate(&elf_with_interp_segment()),
            Err(UserElfError::UnsupportedSegment)
        );
    }

    #[test]
    fn dynamic_segment_is_rejected() {
        assert_eq!(
            validate(&elf_with_dynamic_segment()),
            Err(UserElfError::UnsupportedSegment)
        );
    }

    #[test]
    fn bss_remainder_is_zero_filled() {
        let elf = minimal_user_elf();
        let image = validate(&elf).unwrap();
        let mut destination = [0xA5u8; 4096];

        copy_segment_for_test(&elf, image.segment(1).unwrap(), &mut destination).unwrap();

        assert_eq!(&destination[..4], b"DATA");
        assert!(destination[4..16].iter().all(|&byte| byte == 0));
    }
}
