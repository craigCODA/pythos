//! ADR 0112: one native, read-only QEMU NVMe block request.

use core::arch::asm;
use core::sync::atomic::{Ordering, fence};

use pythos_shared::{boot_protocol::PythBootInfo, sha256::sha256};

use super::{
    DmaBytes, PCI_COMMAND_BUS_MASTER, PCI_COMMAND_MEMORY_SPACE, PCI_COMMAND_OFFSET,
    PCI_CONFIG_ADDRESS, PCI_CONFIG_DATA, SECTOR_SIZE, config_address, dma_physical, outl, outw,
    read_config_u32, read_u8, write_u32,
};
use crate::{
    memory::{
        physical::{PAGE_SIZE, PhysicalMemory},
        r#virtual::{KernelAddressSpace, KernelAddressSpaceBuildOptions, translate_active_address},
    },
    qemu_exit, serial,
    storage_probe::{self, MemoryBar, StorageController, StorageControllerKind},
};

const PREFIX: &str = "PYTHOS:CORE:NVME_READ:";
const QEMU_VENDOR_ID: u16 = 0x1b36;
const QEMU_DEVICE_ID: u16 = 0x0010;
const QEMU_SERIAL: &[u8; 20] = b"PYTHOS-NVME-READ    ";
const MMIO_VIRT: u64 = 0xFFFF_C000_1006_0000;
const MMIO_LEN: u64 = 2 * PAGE_SIZE;
const PCI_INTX_DISABLE: u16 = 1 << 10;
const PCI_CONTROL_BITS: u16 = PCI_INTX_DISABLE | PCI_COMMAND_MEMORY_SPACE | PCI_COMMAND_BUS_MASTER;

const CAP: usize = 0x00;
const VS: usize = 0x08;
const INTMS: usize = 0x0c;
const CC: usize = 0x14;
const CSTS: usize = 0x1c;
const AQA: usize = 0x24;
const ASQ: usize = 0x28;
const ACQ: usize = 0x30;
const DOORBELL: usize = 0x1000;
const CC_ENABLE: u32 = 1;
const CC_QUEUE_ENTRY_SIZES: u32 = (6 << 16) | (4 << 20);
const CSTS_READY: u32 = 1;
const CSTS_FATAL: u32 = 1 << 1;
const CAP_NVM_COMMAND_SET: u64 = 1 << 37;

const QUEUE_DEPTH: u16 = 2;
const SUBMISSION_BYTES: usize = 64;
const COMPLETION_BYTES: usize = 16;
const DMA_PAGE_BYTES: usize = PAGE_SIZE as usize;
const POLL_LIMIT: usize = 4_000_000;
const IO_QUEUE_ID: u16 = 1;
const NAMESPACE_ID: u32 = 1;
const READ_LBA: u64 = 23;

static ADMIN_SUBMISSION: DmaBytes<DMA_PAGE_BYTES> =
    DmaBytes(core::cell::UnsafeCell::new([0; DMA_PAGE_BYTES]));
static ADMIN_COMPLETION: DmaBytes<DMA_PAGE_BYTES> =
    DmaBytes(core::cell::UnsafeCell::new([0; DMA_PAGE_BYTES]));
static IO_SUBMISSION: DmaBytes<DMA_PAGE_BYTES> =
    DmaBytes(core::cell::UnsafeCell::new([0; DMA_PAGE_BYTES]));
static IO_COMPLETION: DmaBytes<DMA_PAGE_BYTES> =
    DmaBytes(core::cell::UnsafeCell::new([0; DMA_PAGE_BYTES]));
static DATA: DmaBytes<DMA_PAGE_BYTES> = DmaBytes(core::cell::UnsafeCell::new([0; DMA_PAGE_BYTES]));

pub(crate) fn run(boot: &PythBootInfo, memory: &mut PhysicalMemory) -> ! {
    mark("ENTER");
    match probe(boot, memory) {
        Ok(()) => {
            mark("COMPLETE");
            qemu_exit::success()
        }
        Err(reason) => {
            serial::write_str(PREFIX);
            serial::write_str("ERROR:");
            serial::write_line(reason);
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic()
        }
    }
}

fn probe(boot: &PythBootInfo, memory: &mut PhysicalMemory) -> Result<(), &'static str> {
    let report = storage_probe::run_probe();
    let mut candidates = (0..report.count())
        .filter_map(|index| report.controller_at(index))
        .filter(|controller| {
            controller.kind == StorageControllerKind::Nvme
                && controller.vendor_id == QEMU_VENDOR_ID
                && controller.device_id == QEMU_DEVICE_ID
        });
    let pci = candidates.next().ok_or("CONTROLLER_ABSENT")?;
    if report.overflowed() || candidates.next().is_some() {
        return Err("CONTROLLER_AMBIGUOUS");
    }
    mark("CONTROLLER_FOUND");

    // The QEMU NVMe BAR includes the controller register page and the first
    // doorbell page. This profile uses only queue pairs 0 and 1 in that window.
    let physical = match pci.bar0 {
        Some(MemoryBar::Memory32(base) | MemoryBar::Memory64(base))
            if base != 0 && base.is_multiple_of(MMIO_LEN) =>
        {
            base
        }
        _ => return Err("BAR0"),
    };
    let last_physical = physical.checked_add(MMIO_LEN - 1).ok_or("BAR0")?;
    let mut options = KernelAddressSpaceBuildOptions::new();
    options.nvme_mmio = Some((physical, MMIO_VIRT, MMIO_LEN));
    let address_space =
        KernelAddressSpace::build(memory, boot, options).map_err(|_| "MMIO_MAPPING")?;

    // SAFETY: build retained PythCore, its current stack, COM1, and boot
    // metadata in this PythCore-owned hierarchy. The root is page aligned
    // and the mapped register window covers two pages. The hierarchy lives
    // until the proof exits. Only the boot CPU runs with IRQs disabled.
    // An invalid hierarchy would fault before controller setup.
    unsafe { address_space.activate() };
    address_space
        .validate_active(boot)
        .map_err(|_| "MMIO_MAPPING")?;
    if !matches!(translate_active_address(MMIO_VIRT), Ok(start) if start == physical)
        || !matches!(translate_active_address(MMIO_VIRT + MMIO_LEN - 1), Ok(end) if end == last_physical)
    {
        return Err("MMIO_MAPPING");
    }
    mark("MMIO_READY");
    read_and_quiesce(pci)
}

fn read_and_quiesce(pci: StorageController) -> Result<(), &'static str> {
    let original = pci_command(pci);
    let stopped_command = original & !PCI_COMMAND_BUS_MASTER;
    let register_access = stopped_command | PCI_COMMAND_MEMORY_SPACE | PCI_INTX_DISABLE;
    if !set_pci_command(pci, register_access) {
        return Err(if set_pci_command(pci, stopped_command) {
            "PCI_ENABLE"
        } else {
            "PCI_QUIESCE"
        });
    }

    let registers = Registers;
    let stride = match registers.doorbell_stride() {
        Ok(stride) => stride,
        Err(reason) => {
            return Err(if set_pci_command(pci, stopped_command) {
                reason
            } else {
                "PCI_QUIESCE"
            });
        }
    };
    let result = read_known_block(registers, stride, pci, register_access);
    let disabled = registers.disable();
    let dma_off = set_pci_command(pci, stopped_command);
    if !dma_off {
        return Err("PCI_QUIESCE");
    }
    disabled.map_err(|_| "CONTROLLER_QUIESCE")?;
    mark("QUIESCED");
    result
}

fn read_known_block(
    registers: Registers,
    stride: usize,
    pci: StorageController,
    register_access: u16,
) -> Result<(), &'static str> {
    registers.disable()?;
    let mut admin = Queue::new(0, stride, &ADMIN_SUBMISSION, &ADMIN_COMPLETION)?;
    let mut io = Queue::new(IO_QUEUE_ID, stride, &IO_SUBMISSION, &IO_COMPLETION)?;
    let data = DmaPage::new(&DATA)?;

    let size = u32::from(QUEUE_DEPTH - 1);
    registers.write(AQA, size | (size << 16));
    registers.write_address(ASQ, admin.submission.physical);
    registers.write_address(ACQ, admin.completion.physical);
    // The QEMU profile uses OVMF's polling setup with MSI-X disabled.
    registers.write(INTMS, 1);
    if !set_pci_command(pci, register_access | PCI_COMMAND_BUS_MASTER) {
        return Err("PCI_BUS_MASTER");
    }
    registers.write(CC, CC_QUEUE_ENTRY_SIZES | CC_ENABLE);
    registers.wait_ready(true)?;
    mark("ADMIN_READY");

    admin.submit(registers, Command::identify(1, 0, data.physical))?;
    if &data.copy_bytes::<20>(4) != QEMU_SERIAL {
        return Err("CONTROLLER_IDENTITY");
    }
    mark("CONTROLLER_IDENTIFIED");

    admin.submit(registers, Command::identify(0, NAMESPACE_ID, data.physical))?;
    let blocks = namespace_blocks(&data.copy_bytes::<192>(0))?;
    number("NAMESPACE=", u64::from(NAMESPACE_ID));
    number("BLOCK_BYTES=", SECTOR_SIZE as u64);
    number("BLOCKS=", blocks);
    if READ_LBA >= blocks {
        return Err("READ_RANGE");
    }

    let mut request_io_queues = Command::new(0x09);
    // Set Features, Number of Queues. A zero count in each half requests
    // one I/O submission queue and one I/O completion queue.
    request_io_queues.0[10] = 0x07;
    admin.submit(registers, request_io_queues)?;
    admin.submit(
        registers,
        Command::create_io_queue(0x05, io.completion.physical, 1),
    )?;
    // Physically contiguous submission queue assigned to completion queue 1.
    let submit_attributes = (u32::from(IO_QUEUE_ID) << 16) | 1;
    admin.submit(
        registers,
        Command::create_io_queue(0x01, io.submission.physical, submit_attributes),
    )?;
    mark("IO_READY");

    io.submit(registers, Command::read_one(READ_LBA, data.physical))?;
    number("READ_LBA=", READ_LBA);
    let digest = sha256(&data.copy_bytes::<SECTOR_SIZE>(0));
    serial::write_str(PREFIX);
    serial::write_str("READ_SHA256=");
    for word in digest.chunks_exact(8) {
        serial::write_hex_u64_value(u64::from_be_bytes(word.try_into().unwrap()));
    }
    serial::write_line("");
    Ok(())
}

fn namespace_blocks(identify: &[u8; 192]) -> Result<u64, &'static str> {
    let blocks = u64::from_le_bytes(identify[..8].try_into().unwrap());
    let capacity = u64::from_le_bytes(identify[8..16].try_into().unwrap());
    if blocks == 0 || capacity == 0 || capacity > blocks {
        return Err("NAMESPACE_CAPACITY");
    }

    // Parse the active NVMe 1.x LBA format. This slice supports plain
    // 512-byte blocks with zero metadata and protection information.
    let format = identify[26];
    if format >= 16 || format > identify[25] || identify[29] & 7 != 0 {
        return Err("NAMESPACE_FORMAT");
    }
    let index = 128 + usize::from(format) * 4;
    let metadata = u16::from_le_bytes(identify[index..index + 2].try_into().unwrap());
    if metadata != 0 || identify[index + 2] != (SECTOR_SIZE.ilog2() as u8) {
        return Err("NAMESPACE_FORMAT");
    }
    Ok(blocks)
}

fn pci_command(controller: StorageController) -> u16 {
    read_config_u32(
        controller.bus,
        controller.device,
        controller.function,
        PCI_COMMAND_OFFSET,
    ) as u16
}

fn set_pci_command(controller: StorageController, command: u16) -> bool {
    outl(
        PCI_CONFIG_ADDRESS,
        config_address(
            controller.bus,
            controller.device,
            controller.function,
            PCI_COMMAND_OFFSET,
        ),
    );
    // The 16-bit write preserves the adjacent PCI status register.
    outw(PCI_CONFIG_DATA, command);
    pci_command(controller) & PCI_CONTROL_BITS == command & PCI_CONTROL_BITS
}

#[derive(Clone, Copy)]
struct Registers;

impl Registers {
    fn read(self, offset: usize) -> u32 {
        assert!(offset.is_multiple_of(4) && offset <= MMIO_LEN as usize - 4);
        // SAFETY: probe validated the supervisor-only BAR0 window in CR3.
        // The aligned dword is within two retained, cache-disabled pages.
        // The pointer addresses device registers, so no Rust references
        // alias the controller. Only the boot CPU touches these offsets.
        // An incorrect mapping could read another device's registers.
        unsafe { ((MMIO_VIRT as usize + offset) as *const u32).read_volatile() }
    }

    fn write(self, offset: usize, value: u32) {
        assert!(offset.is_multiple_of(4) && offset <= MMIO_LEN as usize - 4);
        // SAFETY: the active PythCore root maps the two checked BAR0 pages
        // writable until this proof exits. Offset checks cover a full aligned
        // dword. The controller owns the MMIO region and the sole boot CPU
        // writes its controlled offsets. The assembly retains memory effects
        // because a doorbell store can start DMA to kernel memory. An
        // invalid address could mutate unrelated hardware.
        unsafe {
            asm!(
                "mov dword ptr [{address}], {value:e}",
                address = in(reg) MMIO_VIRT as usize + offset,
                value = in(reg) value,
                options(nostack, preserves_flags),
            );
        }
    }

    fn write_address(self, offset: usize, physical: u64) {
        self.write(offset, physical as u32);
        self.write(offset + 4, (physical >> 32) as u32);
    }

    fn doorbell_stride(self) -> Result<usize, &'static str> {
        let cap = u64::from(self.read(CAP)) | (u64::from(self.read(CAP + 4)) << 32);
        let stride = 4usize << ((cap >> 32) & 0xf);
        let highest_doorbell = DOORBELL + 3 * stride;
        if self.read(VS) >> 16 != 1
            || cap & 0xffff < u64::from(QUEUE_DEPTH - 1)
            || cap & CAP_NVM_COMMAND_SET == 0
            || (cap >> 48) & 0xf != 0
            || highest_doorbell > MMIO_LEN as usize - 4
        {
            return Err("CONTROLLER_CAPABILITIES");
        }
        Ok(stride)
    }

    fn disable(self) -> Result<(), &'static str> {
        let configuration = self.read(CC);
        if configuration & CC_ENABLE != 0 {
            self.write(CC, configuration & !CC_ENABLE);
        }
        self.wait_ready(false)
    }

    fn wait_ready(self, ready: bool) -> Result<(), &'static str> {
        for _ in 0..POLL_LIMIT {
            let status = self.read(CSTS);
            if status == u32::MAX || (ready && status & CSTS_FATAL != 0) {
                return Err("CONTROLLER_STATUS");
            }
            if (status & CSTS_READY != 0) == ready {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err("CONTROLLER_TIMEOUT")
    }
}

struct DmaPage {
    base: *mut u8,
    physical: u64,
}

impl DmaPage {
    fn new(page: &'static DmaBytes<DMA_PAGE_BYTES>) -> Result<Self, &'static str> {
        let base = page.0.get().cast::<u8>();
        let physical = dma_physical(base as u64).map_err(|_| "DMA_PAGE")?;
        if physical == 0 || !physical.is_multiple_of(PAGE_SIZE) {
            return Err("DMA_PAGE");
        }
        Ok(Self { base, physical })
    }

    fn copy_bytes<const N: usize>(&self, offset: usize) -> [u8; N] {
        assert!(offset <= DMA_PAGE_BYTES && N <= DMA_PAGE_BYTES - offset);
        core::array::from_fn(|index| read_u8(self.base, offset + index))
    }

    fn read_completion_dword(&self, offset: usize) -> u32 {
        assert!(offset.is_multiple_of(4) && offset <= DMA_PAGE_BYTES - 4);
        let value: u32;
        // SAFETY: Queue owns this aligned static completion page for the
        // lifetime of the proof, and offset bounds cover one mapped dword.
        // The controller publishes the completion, and the sole host CPU
        // snapshots it with one aligned x86 load and no Rust reference to
        // the live DMA bytes. A bytewise read could split phase and status.
        unsafe {
            asm!(
                "mov {value:e}, dword ptr [{address}]",
                value = out(reg) value,
                address = in(reg) self.base.wrapping_add(offset),
                options(nostack, preserves_flags),
            );
        }
        value
    }
}

struct Command([u32; 16]);

impl Command {
    fn new(opcode: u8) -> Self {
        let mut words = [0; 16];
        words[0] = u32::from(opcode);
        Self(words)
    }

    fn set_prp(&mut self, physical: u64) {
        self.0[6] = physical as u32;
        self.0[7] = (physical >> 32) as u32;
    }

    fn identify(selector: u32, namespace: u32, physical: u64) -> Self {
        let mut command = Self::new(0x06);
        command.0[1] = namespace;
        command.set_prp(physical);
        command.0[10] = selector;
        command
    }

    fn create_io_queue(opcode: u8, physical: u64, attributes: u32) -> Self {
        let mut command = Self::new(opcode);
        command.set_prp(physical);
        command.0[10] = u32::from(IO_QUEUE_ID) | (u32::from(QUEUE_DEPTH - 1) << 16);
        command.0[11] = attributes;
        command
    }

    fn read_one(lba: u64, physical: u64) -> Self {
        let mut command = Self::new(0x02);
        command.0[1] = NAMESPACE_ID;
        command.set_prp(physical);
        command.0[10] = lba as u32;
        command.0[11] = (lba >> 32) as u32;
        // CDW12.NLB is zero for one logical block.
        command
    }
}

struct Queue {
    id: u16,
    submission: DmaPage,
    completion: DmaPage,
    tail: u16,
    head: u16,
    phase: bool,
    next_cid: u16,
    submission_doorbell: usize,
    completion_doorbell: usize,
}

impl Queue {
    fn new(
        id: u16,
        stride: usize,
        submission: &'static DmaBytes<DMA_PAGE_BYTES>,
        completion: &'static DmaBytes<DMA_PAGE_BYTES>,
    ) -> Result<Self, &'static str> {
        let doorbell = DOORBELL + 2 * usize::from(id) * stride;
        Ok(Self {
            id,
            submission: DmaPage::new(submission)?,
            completion: DmaPage::new(completion)?,
            tail: 0,
            head: 0,
            // Controller phase starts at one against the loader-zeroed page.
            phase: true,
            next_cid: 1,
            submission_doorbell: doorbell,
            completion_doorbell: doorbell + stride,
        })
    }

    fn submit(&mut self, registers: Registers, mut command: Command) -> Result<(), &'static str> {
        let cid = self.next_cid;
        self.next_cid = self.next_cid.wrapping_add(1);
        command.0[0] |= u32::from(cid) << 16;
        let entry = usize::from(self.tail) * SUBMISSION_BYTES;
        for (index, word) in command.0.into_iter().enumerate() {
            write_u32(self.submission.base, entry + index * 4, word);
        }
        self.tail = (self.tail + 1) % QUEUE_DEPTH;
        fence(Ordering::SeqCst);
        registers.write(self.submission_doorbell, u32::from(self.tail));

        let entry = usize::from(self.head) * COMPLETION_BYTES;
        for _ in 0..POLL_LIMIT {
            let footer = self.completion.read_completion_dword(entry + 12);
            let status = (footer >> 16) as u16;
            if (status & 1 != 0) == self.phase {
                fence(Ordering::Acquire);
                let route = self.completion.read_completion_dword(entry + 8);
                if footer as u16 != cid
                    || (route >> 16) as u16 != self.id
                    || route as u16 != self.tail
                {
                    return Err("COMPLETION_IDENTITY");
                }
                self.head = (self.head + 1) % QUEUE_DEPTH;
                if self.head == 0 {
                    self.phase = !self.phase;
                }
                registers.write(self.completion_doorbell, u32::from(self.head));
                if status & 0x0ffe != 0 {
                    number("COMMAND_STATUS=", u64::from(status >> 1));
                    return Err("COMMAND_STATUS");
                }
                return Ok(());
            }
            if registers.read(CSTS) & CSTS_FATAL != 0 {
                return Err("CONTROLLER_STATUS");
            }
            core::hint::spin_loop();
        }
        Err("COMMAND_TIMEOUT")
    }
}

fn mark(message: &str) {
    serial::write_str(PREFIX);
    serial::write_line(message);
}

fn number(name: &str, value: u64) {
    serial::write_str(PREFIX);
    serial::write_hex_u64(name, value);
}
