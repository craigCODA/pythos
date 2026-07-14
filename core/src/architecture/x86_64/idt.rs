//! Minimal x86-64 IDT installation.

use crate::architecture::x86_64::{exceptions, gdt};
use core::arch::asm;
use core::cell::UnsafeCell;

const IDT_ENTRIES: usize = 256;
const INTERRUPT_GATE_PRESENT: u8 = 0x8E;

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            attributes: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    fn new(handler: u64, selector: u16) -> Self {
        Self {
            offset_low: handler as u16,
            selector,
            ist: 0,
            attributes: INTERRUPT_GATE_PRESENT,
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }

    #[cfg(test)]
    fn handler(&self) -> u64 {
        u64::from(self.offset_low)
            | (u64::from(self.offset_mid) << 16)
            | (u64::from(self.offset_high) << 32)
    }
}

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

struct IdtStorage(UnsafeCell<[IdtEntry; IDT_ENTRIES]>);

// SAFETY: the IDT is initialized once during single-core milestone-1 boot,
// before interrupts are enabled and before any concurrent readers exist.
unsafe impl Sync for IdtStorage {}

static IDT: IdtStorage = IdtStorage(UnsafeCell::new([IdtEntry::missing(); IDT_ENTRIES]));

pub fn initialize() -> Result<(), ()> {
    // SAFETY:
    // 1. Invariant: the static IDT is written once before `lidt` publishes it.
    // 2. Established by: milestone-1 boot is single-core with interrupts disabled.
    // 3. Lifetime: the static IDT remains mapped for the rest of PythCore.
    // 4. Pointer ownership: PythCore exclusively owns static IDT storage.
    // 5. Alignment: `UnsafeCell<[IdtEntry; N]>` preserves entry alignment.
    // 6. Mapped length: exactly 256 IDT entries are written.
    // 7. Concurrency: no other CPU or interrupt handler reads it during setup.
    // 8. Violation: a partially initialized IDT would fault on exceptions.
    let idt = unsafe { &mut *IDT.0.get() };
    for (index, entry) in idt.iter_mut().enumerate() {
        let handler = if index < 32 {
            exceptions::handler_for_vector(index)
        } else {
            exceptions::panic_stub as *const () as usize as u64
        };
        *entry = IdtEntry::new(handler, gdt::KERNEL_CODE_SELECTOR);
    }

    let idtr = DescriptorTablePointer {
        limit: (core::mem::size_of_val(idt) - 1) as u16,
        base: idt.as_ptr() as u64,
    };

    // SAFETY:
    // 1. Invariant: `idtr` points to a fully initialized static IDT whose
    //    entries use the loaded kernel code selector and a mapped panic stub.
    // 2. Established by: GDT initialization ran before this function and the
    //    IDT entries were filled immediately above.
    // 3. Lifetime: IDT storage and handler code remain mapped for all PythCore.
    // 4. Pointer ownership: CPU descriptor register borrows PythCore-owned data.
    // 5. Alignment: packed `idtr` layout matches `lidt`; entries have valid layout.
    // 6. Mapped length: `idtr.limit + 1` bytes cover all 256 IDT entries.
    // 7. Concurrency: single-core boot with interrupts disabled.
    // 8. Violation: bad IDT data would route exceptions to invalid code.
    unsafe {
        asm!("lidt [{idtr}]", idtr = in(reg) &idtr, options(readonly, nostack, preserves_flags));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idt_entry_encodes_handler_selector_and_interrupt_gate_attributes() {
        let entry = IdtEntry::new(0xFFFF_FFFF_8123_4567, gdt::KERNEL_CODE_SELECTOR);
        let selector = entry.selector;
        let ist = entry.ist;
        let attributes = entry.attributes;
        let reserved = entry.reserved;

        assert_eq!(entry.handler(), 0xFFFF_FFFF_8123_4567);
        assert_eq!(selector, gdt::KERNEL_CODE_SELECTOR);
        assert_eq!(ist, 0);
        assert_eq!(attributes, INTERRUPT_GATE_PRESENT);
        assert_eq!(reserved, 0);
    }
}
