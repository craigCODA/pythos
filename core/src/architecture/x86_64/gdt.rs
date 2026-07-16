//! Minimal x86-64 GDT and TSS installation.

use crate::architecture::x86_64::tss;
use core::arch::asm;
use core::cell::UnsafeCell;

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
const KERNEL_DATA_SELECTOR: u16 = 0x10;
const TSS_SELECTOR: u16 = 0x18;
pub const USER_DATA_SELECTOR: u16 = 0x2B;
pub const USER_CODE_SELECTOR: u16 = 0x33;

const GDT_NULL: u64 = 0;
const GDT_KERNEL_CODE: u64 = 0x00AF_9A00_0000_FFFF;
const GDT_KERNEL_DATA: u64 = 0x00AF_9200_0000_FFFF;
const GDT_USER_DATA: u64 = 0x00AF_F200_0000_FFFF;
const GDT_USER_CODE: u64 = 0x00AF_FA00_0000_FFFF;

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

struct GdtStorage(UnsafeCell<[u64; 7]>);

// SAFETY: the GDT is initialized once during single-core milestone-1 boot,
// before interrupts are enabled and before any concurrent readers exist.
unsafe impl Sync for GdtStorage {}

static GDT: GdtStorage = GdtStorage(UnsafeCell::new([0; 7]));

pub fn initialize() -> Result<(), ()> {
    let tss_descriptor = encode_tss_descriptor(tss::base(), tss::limit());
    // SAFETY:
    // 1. Invariant: the static GDT is written once before `lgdt` publishes it.
    // 2. Established by: milestone-1 boot is single-core with interrupts still disabled.
    // 3. Lifetime: the static GDT remains mapped for the rest of PythCore.
    // 4. Pointer ownership: PythCore exclusively owns the static GDT storage.
    // 5. Alignment: `UnsafeCell<[u64; 5]>` preserves `u64` alignment.
    // 6. Mapped length: exactly five eight-byte GDT slots are written.
    // 7. Concurrency: no other CPU or interrupt handler reads it during setup.
    // 8. Violation: a partially initialized GDT would fault on segment reload.
    let gdt = unsafe { &mut *GDT.0.get() };
    gdt[0] = GDT_NULL;
    gdt[1] = GDT_KERNEL_CODE;
    gdt[2] = GDT_KERNEL_DATA;
    gdt[3] = tss_descriptor.low;
    gdt[4] = tss_descriptor.high;
    gdt[5] = GDT_USER_DATA;
    gdt[6] = GDT_USER_CODE;

    let gdtr = DescriptorTablePointer {
        limit: (core::mem::size_of_val(gdt) - 1) as u16,
        base: gdt.as_ptr() as u64,
    };

    // SAFETY:
    // 1. Invariant: `gdtr` points to the initialized static GDT, selectors
    //    reference present kernel code/data/TSS descriptors, and the TSS base
    //    points to a valid static TSS.
    // 2. Established by: the assignments above and `tss::base()/limit()`.
    // 3. Lifetime: GDT and TSS are static and stay mapped after this function.
    // 4. Pointer ownership: CPU descriptor registers borrow PythCore-owned static data.
    // 5. Alignment: `gdtr` is naturally aligned for its packed layout; GDT
    //    entries are eight-byte aligned.
    // 6. Mapped length: `gdtr.limit + 1` bytes cover all five GDT entries.
    // 7. Concurrency: single-core boot with interrupts disabled.
    // 8. Violation: bad descriptors or selectors would trigger a fault before
    //    the IDT slice exists.
    unsafe {
        asm!(
            "lgdt [{gdtr}]",
            "push {code_selector}",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, {data_selector}",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            "mov ax, {tss_selector}",
            "ltr ax",
            gdtr = in(reg) &gdtr,
            code_selector = const KERNEL_CODE_SELECTOR,
            data_selector = const KERNEL_DATA_SELECTOR,
            tss_selector = const TSS_SELECTOR,
            out("rax") _,
        );
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TssDescriptor {
    low: u64,
    high: u64,
}

fn encode_tss_descriptor(base: u64, limit: u32) -> TssDescriptor {
    let low = u64::from(limit & 0xFFFF)
        | ((base & 0x00FF_FFFF) << 16)
        | (0x89u64 << 40)
        | (u64::from((limit >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56);
    let high = base >> 32;
    TssDescriptor { low, high }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tss_descriptor_encodes_base_limit_and_present_available_tss_type() {
        let descriptor = encode_tss_descriptor(0xFFFF_FFFF_8123_4000, 0x67);

        assert_eq!(descriptor.low & 0xFFFF, 0x67);
        assert_eq!((descriptor.low >> 40) & 0xFF, 0x89);
        assert_eq!(
            ((descriptor.low >> 16) & 0x00FF_FFFF)
                | ((descriptor.low >> 56) << 24)
                | (descriptor.high << 32),
            0xFFFF_FFFF_8123_4000
        );
    }

    #[test]
    fn user_selectors_are_ring3_segments_after_tss_descriptor() {
        assert_eq!(USER_DATA_SELECTOR & 0x3, 0x3);
        assert_eq!(USER_CODE_SELECTOR & 0x3, 0x3);
        assert_eq!(USER_DATA_SELECTOR, 0x2B);
        assert_eq!(USER_CODE_SELECTOR, 0x33);
    }
}
