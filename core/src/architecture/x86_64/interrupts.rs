//! Legacy PIC interrupt-controller setup for Phase 2 bootstrap.
#![cfg_attr(test, allow(dead_code))]

use core::arch::asm;

use crate::architecture::x86_64::timer;
#[cfg(not(test))]
use crate::ps2;
#[cfg(not(test))]
use crate::scheduler;

/// IRQ1 (keyboard) after PIC remap, per `vector_for_irq`.
const VECTOR_IRQ1_KEYBOARD: u64 = PIC_MASTER_OFFSET as u64 + 1;
/// IRQ12 (mouse) after PIC remap, per `vector_for_irq`.
const VECTOR_IRQ12_MOUSE: u64 = PIC_SLAVE_OFFSET as u64 + (12 - 8);

#[cfg(not(test))]
use core::arch::global_asm;

pub const PIC_MASTER_OFFSET: u8 = 0x20;
pub const PIC_SLAVE_OFFSET: u8 = 0x28;
const PIC_IRQ_COUNT: u8 = 16;

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const ICW1_INIT: u8 = 0x10;
const ICW1_ICW4: u8 = 0x01;
const ICW4_8086: u8 = 0x01;
const PIC_EOI: u8 = 0x20;

#[repr(C)]
pub struct InterruptFrame {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
    vector: u64,
    error_code: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

#[cfg(not(test))]
global_asm!(
    r#"
    .macro IRQ_STUB vector
    .global interrupt_stub_\vector
    interrupt_stub_\vector:
        push 0
        push \vector
        jmp external_interrupt_common
    .endm

    IRQ_STUB 32
    IRQ_STUB 33
    IRQ_STUB 34
    IRQ_STUB 35
    IRQ_STUB 36
    IRQ_STUB 37
    IRQ_STUB 38
    IRQ_STUB 39
    IRQ_STUB 40
    IRQ_STUB 41
    IRQ_STUB 42
    IRQ_STUB 43
    IRQ_STUB 44
    IRQ_STUB 45
    IRQ_STUB 46
    IRQ_STUB 47

    .global external_interrupt_common
    external_interrupt_common:
        push rax
        push rbx
        push rcx
        push rdx
        push rbp
        push rdi
        push rsi
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15
        mov rdi, rsp
        mov r12, rsp
        and rsp, -16
        call external_interrupt_handler
        mov rsp, r12
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rsi
        pop rdi
        pop rbp
        pop rdx
        pop rcx
        pop rbx
        pop rax
        add rsp, 16
        iretq
    "#
);

#[cfg(not(test))]
unsafe extern "C" {
    fn interrupt_stub_32();
    fn interrupt_stub_33();
    fn interrupt_stub_34();
    fn interrupt_stub_35();
    fn interrupt_stub_36();
    fn interrupt_stub_37();
    fn interrupt_stub_38();
    fn interrupt_stub_39();
    fn interrupt_stub_40();
    fn interrupt_stub_41();
    fn interrupt_stub_42();
    fn interrupt_stub_43();
    fn interrupt_stub_44();
    fn interrupt_stub_45();
    fn interrupt_stub_46();
    fn interrupt_stub_47();
}

pub fn initialize() -> Result<(), ()> {
    remap_pic();
    mask_all();
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub const fn vector_for_irq(irq: u8) -> Option<u8> {
    if irq < 8 {
        Some(PIC_MASTER_OFFSET + irq)
    } else if irq < PIC_IRQ_COUNT {
        Some(PIC_SLAVE_OFFSET + (irq - 8))
    } else {
        None
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn mask_irq(irq: u8) -> Result<(), ()> {
    set_irq_mask(irq, true)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn unmask_irq(irq: u8) -> Result<(), ()> {
    set_irq_mask(irq, false)
}

fn set_irq_mask(irq: u8, masked: bool) -> Result<(), ()> {
    if irq >= PIC_IRQ_COUNT {
        return Err(());
    }
    let port = if irq < 8 { PIC1_DATA } else { PIC2_DATA };
    let bit = 1u8 << (irq % 8);
    let current = inb(port);
    let updated = if masked {
        current | bit
    } else {
        current & !bit
    };
    outb(port, updated);
    Ok(())
}

pub fn handler_for_vector(vector: usize) -> u64 {
    #[cfg(test)]
    {
        let _ = vector;
        external_interrupt_handler as *const () as usize as u64
    }

    #[cfg(not(test))]
    {
        match vector {
            32 => interrupt_stub_32 as *const () as usize as u64,
            33 => interrupt_stub_33 as *const () as usize as u64,
            34 => interrupt_stub_34 as *const () as usize as u64,
            35 => interrupt_stub_35 as *const () as usize as u64,
            36 => interrupt_stub_36 as *const () as usize as u64,
            37 => interrupt_stub_37 as *const () as usize as u64,
            38 => interrupt_stub_38 as *const () as usize as u64,
            39 => interrupt_stub_39 as *const () as usize as u64,
            40 => interrupt_stub_40 as *const () as usize as u64,
            41 => interrupt_stub_41 as *const () as usize as u64,
            42 => interrupt_stub_42 as *const () as usize as u64,
            43 => interrupt_stub_43 as *const () as usize as u64,
            44 => interrupt_stub_44 as *const () as usize as u64,
            45 => interrupt_stub_45 as *const () as usize as u64,
            46 => interrupt_stub_46 as *const () as usize as u64,
            47 => interrupt_stub_47 as *const () as usize as u64,
            _ => external_interrupt_handler as *const () as usize as u64,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn external_interrupt_handler(frame: &mut InterruptFrame) {
    if (PIC_MASTER_OFFSET as u64..(PIC_SLAVE_OFFSET as u64 + 8)).contains(&frame.vector) {
        if frame.vector == PIC_MASTER_OFFSET as u64 {
            timer::handle_timer_interrupt();
            send_eoi(frame.vector as u8);
            #[cfg(not(test))]
            scheduler::handle_timer_preemption();
            return;
        } else if frame.vector == VECTOR_IRQ1_KEYBOARD {
            #[cfg(not(test))]
            ps2::handle_keyboard_interrupt();
            send_eoi(frame.vector as u8);
            return;
        } else if frame.vector == VECTOR_IRQ12_MOUSE {
            #[cfg(not(test))]
            ps2::handle_mouse_interrupt();
            send_eoi(frame.vector as u8);
            return;
        }
        send_eoi(frame.vector as u8);
    }
}

fn send_eoi(vector: u8) {
    let Some(irq) = vector.checked_sub(PIC_MASTER_OFFSET) else {
        return;
    };
    if irq >= PIC_IRQ_COUNT {
        return;
    }

    if irq >= 8 {
        outb(PIC2_COMMAND, PIC_EOI);
    }
    outb(PIC1_COMMAND, PIC_EOI);
}

fn remap_pic() {
    let command = ICW1_INIT | ICW1_ICW4;
    outb(PIC1_COMMAND, command);
    io_wait();
    outb(PIC2_COMMAND, command);
    io_wait();
    outb(PIC1_DATA, PIC_MASTER_OFFSET);
    io_wait();
    outb(PIC2_DATA, PIC_SLAVE_OFFSET);
    io_wait();
    outb(PIC1_DATA, 0x04);
    io_wait();
    outb(PIC2_DATA, 0x02);
    io_wait();
    outb(PIC1_DATA, ICW4_8086);
    io_wait();
    outb(PIC2_DATA, ICW4_8086);
    io_wait();
}

fn mask_all() {
    outb(PIC1_DATA, 0xFF);
    outb(PIC2_DATA, 0xFF);
}

fn outb(port: u16, value: u8) {
    // SAFETY:
    // 1. Invariant: `port` names a legacy PIC or POST-delay I/O port.
    // 2. Established by: this private helper is only called with constants above.
    // 3. Lifetime: each write completes before the helper returns.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: Phase 2 remains single-core during controller setup.
    // 8. Violation: writing the wrong port can reconfigure unrelated hardware.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY:
    // 1. Invariant: `port` names a legacy PIC data port.
    // 2. Established by: this private helper is only used for PIC mask ports.
    // 3. Lifetime: the read completes before the helper returns.
    // 4. Pointer ownership: no pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: Phase 2 remains single-core when mask state is changed.
    // 8. Violation: reading the wrong port can observe unrelated hardware state.
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

fn io_wait() {
    outb(0x80, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pic_irq_vectors_are_remapped_out_of_exception_range() {
        assert_eq!(vector_for_irq(0), Some(0x20));
        assert_eq!(vector_for_irq(7), Some(0x27));
        assert_eq!(vector_for_irq(8), Some(0x28));
        assert_eq!(vector_for_irq(15), Some(0x2F));
        assert_eq!(vector_for_irq(16), None);
    }

    #[test]
    fn interrupt_frame_layout_matches_hardened_entry_prefix() {
        assert_eq!(core::mem::offset_of!(InterruptFrame, r15), 0);
        assert_eq!(core::mem::offset_of!(InterruptFrame, rax), 112);
        assert_eq!(core::mem::offset_of!(InterruptFrame, vector), 120);
        assert_eq!(core::mem::offset_of!(InterruptFrame, error_code), 128);
        assert_eq!(core::mem::offset_of!(InterruptFrame, rip), 136);
    }
}
