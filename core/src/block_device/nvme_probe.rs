//! Polled NVMe transport for the read and world checkpoint profiles.

#[cfg(feature = "world-session-nvme")]
mod world_disk;
#[cfg(feature = "world-session-graph-window")]
pub(crate) use world_disk::RecordCompletion;
#[cfg(feature = "world-session-nvme")]
pub(crate) use world_disk::WorldDisk;
#[cfg(feature = "world-session-nvme-target")]
mod target;
#[cfg(feature = "world-session-nvme-usb")]
pub(crate) use target::disable_pci_message_interrupts;
#[cfg(feature = "world-session-nvme-target")]
pub(crate) const EMULATED_TARGET: bool = target::TARGET.emulated;

use core::arch::asm;
#[cfg(feature = "world-session-graph-window")]
use core::sync::atomic::AtomicBool;
use core::sync::atomic::{Ordering, fence};

#[cfg(feature = "nvme-read-probe")]
use pythos_shared::{boot_protocol::PythBootInfo, sha256::sha256};

use super::{
    DmaBytes, PCI_COMMAND_BUS_MASTER, PCI_COMMAND_MEMORY_SPACE, PCI_COMMAND_OFFSET,
    PCI_CONFIG_ADDRESS, PCI_CONFIG_DATA, SECTOR_SIZE, config_address, dma_physical, outl, outw,
    read_config_u32, read_u8, write_u32,
};
use crate::{
    memory::{
        physical::PAGE_SIZE,
        r#virtual::{PhysicalDeviceMapping, translate_active_address},
    },
    serial,
    storage_probe::{self, MemoryBar, StorageController, StorageControllerKind},
};
#[cfg(feature = "nvme-read-probe")]
use crate::{
    memory::{
        physical::PhysicalMemory,
        r#virtual::{KernelAddressSpace, KernelAddressSpaceBuildOptions},
    },
    qemu_exit,
};

#[cfg(feature = "nvme-read-probe")]
const PREFIX: &str = "PYTHOS:CORE:NVME_READ:";
#[cfg(feature = "world-session-nvme")]
const PREFIX: &str = "PYTHOS:CORE:NVME_WORLD:";
#[cfg(not(feature = "world-session-nvme-target"))]
const QEMU_VENDOR_ID: u16 = 0x1b36;
#[cfg(not(feature = "world-session-nvme-target"))]
const QEMU_DEVICE_ID: u16 = 0x0010;
#[cfg(feature = "nvme-read-probe")]
const QEMU_SERIAL: &[u8; 20] = b"PYTHOS-NVME-READ    ";
#[cfg(all(
    feature = "world-session-nvme",
    not(feature = "world-session-nvme-target")
))]
const QEMU_SERIAL: &[u8; 20] = b"PYTHOS-NVME-WORLD   ";
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
const IO_TIMEOUT_MILLISECONDS: u64 = 5000;
const IO_QUEUE_ID: u16 = 1;
const NAMESPACE_ID: u32 = 1;
#[cfg(feature = "nvme-read-probe")]
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
// Queue initialization uses the loader-zeroed DMA pages. The graph proof
// owns them for one controller session per boot, including through teardown.
#[cfg(feature = "world-session-graph-window")]
static GRAPH_WORLD_CLAIMED: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "nvme-read-probe")]
pub(crate) fn run(boot: &PythBootInfo, memory: &mut PhysicalMemory) -> ! {
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

#[cfg(feature = "nvme-read-probe")]
fn probe(boot: &PythBootInfo, memory: &mut PhysicalMemory) -> Result<(), &'static str> {
    let location = NvmeLocation::discover()?;
    let mut options = KernelAddressSpaceBuildOptions::new();
    options.nvme_mmio = Some(location.mapping());
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
    location.with_namespace(READ_LBA, |mut namespace| {
        let bytes = namespace.read_sector(READ_LBA)?;
        number("READ_LBA=", READ_LBA);
        let digest = sha256(&bytes);
        serial::write_str(PREFIX);
        serial::write_str("READ_SHA256=");
        for word in digest.chunks_exact(8) {
            serial::write_hex_u64_value(u64::from_be_bytes(word.try_into().unwrap()));
        }
        serial::write_line("");
        Ok(())
    })
}

pub(crate) struct NvmeLocation {
    pci: StorageController,
    physical: u64,
}

impl NvmeLocation {
    pub(crate) fn discover() -> Result<Self, &'static str> {
        mark("ENTER");
        let report = storage_probe::run_probe();
        let mut candidates = (0..report.count())
            .filter_map(|index| report.controller_at(index))
            .filter(|controller| {
                controller.kind == StorageControllerKind::Nvme && matches_pci(*controller)
            });
        let pci = candidates.next().ok_or("CONTROLLER_ABSENT")?;
        if report.overflowed() || candidates.next().is_some() {
            return Err("CONTROLLER_AMBIGUOUS");
        }
        mark("CONTROLLER_FOUND");

        // Each selected BAR covers the controller register page and the
        // first doorbell page. The CAP check bounds the actual doorbells.
        let physical = match pci.bar0 {
            Some(MemoryBar::Memory32(base) | MemoryBar::Memory64(base))
                if base != 0
                    && base.is_multiple_of(MMIO_LEN)
                    && base.checked_add(MMIO_LEN - 1).is_some() =>
            {
                base
            }
            _ => return Err("BAR0"),
        };
        Ok(Self { pci, physical })
    }

    pub(crate) fn mapping(&self) -> PhysicalDeviceMapping {
        (self.physical, MMIO_VIRT, MMIO_LEN)
    }

    #[cfg(all(
        feature = "world-session-nvme",
        not(feature = "world-session-graph-window")
    ))]
    pub(crate) fn with_world_disk(
        self,
        run: impl FnOnce(WorldDisk) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        self.with_namespace(1, |namespace| run(WorldDisk::open(namespace)?))
    }

    #[cfg(feature = "world-session-graph-window")]
    pub(crate) fn with_world_disk(
        self,
        run: impl FnOnce(&mut WorldDisk) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        if GRAPH_WORLD_CLAIMED.swap(true, Ordering::AcqRel) {
            return Err("WORLD_DISK_ALREADY_CLAIMED");
        }
        self.with_namespace(1, |namespace| {
            let mut disk = WorldDisk::open(namespace)?;
            // The borrow ends before controller disable and bus-master
            // teardown on either callback result.
            run(&mut disk)
        })
    }

    fn with_namespace(
        self,
        last_required_lba: u64,
        run: impl FnOnce(NvmeNamespace) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        let last_physical = self.physical + (MMIO_LEN - 1);
        if !matches!(translate_active_address(MMIO_VIRT), Ok(start) if start == self.physical)
            || !matches!(translate_active_address(MMIO_VIRT + MMIO_LEN - 1), Ok(end) if end == last_physical)
        {
            return Err("MMIO_MAPPING");
        }
        mark("MMIO_READY");
        #[cfg(feature = "world-session-nvme-target")]
        target::start_clock()?;
        use_namespace_and_quiesce(self.pci, last_required_lba, run)
    }
}

fn use_namespace_and_quiesce(
    pci: StorageController,
    last_required_lba: u64,
    run: impl FnOnce(NvmeNamespace) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
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
    #[cfg(feature = "world-session-nvme-target")]
    if let Err(reason) = target::disable_message_interrupts(pci) {
        return Err(if set_pci_command(pci, stopped_command) {
            reason
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
    let result =
        open_namespace(registers, stride, pci, register_access, last_required_lba).and_then(run);
    let disabled = registers.disable();
    let dma_off = set_pci_command(pci, stopped_command);
    if !dma_off {
        return Err("PCI_QUIESCE");
    }
    disabled.map_err(|_| "CONTROLLER_QUIESCE")?;
    mark("QUIESCED");
    result
}

fn open_namespace(
    registers: Registers,
    stride: usize,
    pci: StorageController,
    register_access: u16,
    last_required_lba: u64,
) -> Result<NvmeNamespace, &'static str> {
    registers.disable()?;
    let mut admin = Queue::new(0, stride, &ADMIN_SUBMISSION, &ADMIN_COMPLETION)?;
    let io = Queue::new(IO_QUEUE_ID, stride, &IO_SUBMISSION, &IO_COMPLETION)?;
    let data = DmaPage::new(&DATA)?;

    let size = u32::from(QUEUE_DEPTH - 1);
    registers.write(AQA, size | (size << 16));
    registers.write_address(ASQ, admin.submission.physical);
    registers.write_address(ACQ, admin.completion.physical);
    // The controller uses polling and leaves the completion interrupts masked.
    registers.write(INTMS, 1);
    if !set_pci_command(pci, register_access | PCI_COMMAND_BUS_MASTER) {
        return Err("PCI_BUS_MASTER");
    }
    registers.write(CC, CC_QUEUE_ENTRY_SIZES | CC_ENABLE);
    registers.wait_ready(true)?;
    mark("ADMIN_READY");

    admin.submit(registers, Command::identify(1, 0, data.physical))?;
    #[cfg(feature = "world-session-nvme-target")]
    let expected_serial = &target::TARGET.serial;
    #[cfg(not(feature = "world-session-nvme-target"))]
    let expected_serial = QEMU_SERIAL;
    if &data.copy_bytes::<20>(4) != expected_serial {
        return Err("CONTROLLER_IDENTITY");
    }
    mark("CONTROLLER_IDENTIFIED");

    admin.submit(registers, Command::identify(0, NAMESPACE_ID, data.physical))?;
    let blocks = namespace_blocks(&data.copy_bytes::<192>(0))?;
    number("NAMESPACE=", u64::from(NAMESPACE_ID));
    number("BLOCK_BYTES=", SECTOR_SIZE as u64);
    number("BLOCKS=", blocks);
    #[cfg(feature = "world-session-nvme-target")]
    if blocks != target::TARGET.namespace_blocks {
        return Err("NAMESPACE_TARGET_SIZE");
    }
    if last_required_lba >= blocks {
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

    Ok(NvmeNamespace {
        registers,
        io,
        data,
        blocks,
    })
}

struct NvmeNamespace {
    registers: Registers,
    io: Queue,
    data: DmaPage,
    blocks: u64,
}

impl NvmeNamespace {
    fn read_sector(&mut self, lba: u64) -> Result<[u8; SECTOR_SIZE], &'static str> {
        let command = self.read_command(lba)?;
        self.io.submit(self.registers, command)?;
        Ok(self.data.copy_bytes(0))
    }

    fn read_command(&self, lba: u64) -> Result<Command, &'static str> {
        if lba >= self.blocks {
            return Err("READ_RANGE");
        }
        Ok(Command::transfer(0x02, lba, 1, self.data.physical))
    }

    #[cfg(all(
        feature = "world-session-nvme",
        not(feature = "world-session-graph-window")
    ))]
    fn write_page_and_flush(
        &mut self,
        lba: u64,
        record: &[u8; SECTOR_SIZE],
    ) -> Result<(), &'static str> {
        let write = self.prepare_page_write(lba, record)?;
        self.io.submit(self.registers, write)?;
        number("WRITE_LBA=", lba);
        self.io.submit(self.registers, Command::flush())?;
        number("FLUSHED_LBA=", lba);
        Ok(())
    }

    #[cfg(feature = "world-session-nvme")]
    fn prepare_page_write(
        &mut self,
        lba: u64,
        record: &[u8; SECTOR_SIZE],
    ) -> Result<Command, &'static str> {
        self.io.require_idle()?;
        let page_blocks = (DMA_PAGE_BYTES / SECTOR_SIZE) as u16;
        if !lba.is_multiple_of(u64::from(page_blocks))
            || lba
                .checked_add(u64::from(page_blocks))
                .is_none_or(|end| end > self.blocks)
        {
            return Err("WRITE_RANGE");
        }

        // The idle queue owns this aligned DMA page. Fill it before submission
        // and leave it intact while the controller owns the request.
        for (index, bytes) in record.chunks_exact(4).enumerate() {
            write_u32(
                self.data.base,
                index * 4,
                u32::from_le_bytes(bytes.try_into().unwrap()),
            );
        }
        for offset in (SECTOR_SIZE..DMA_PAGE_BYTES).step_by(4) {
            write_u32(self.data.base, offset, 0);
        }
        Ok(Command::transfer(
            0x01,
            lba,
            page_blocks,
            self.data.physical,
        ))
    }
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

fn matches_pci(controller: StorageController) -> bool {
    #[cfg(feature = "world-session-nvme-target")]
    {
        target::matches_pci(controller)
    }
    #[cfg(not(feature = "world-session-nvme-target"))]
    {
        controller.vendor_id == QEMU_VENDOR_ID && controller.device_id == QEMU_DEVICE_ID
    }
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
        let timeout_units = u64::from(self.read(CAP) >> 24).max(1);
        poll(timeout_units * 500, "CONTROLLER_TIMEOUT", || {
            let status = self.read(CSTS);
            if status == u32::MAX || (ready && status & CSTS_FATAL != 0) {
                return Err("CONTROLLER_STATUS");
            }
            Ok(((status & CSTS_READY != 0) == ready).then_some(()))
        })
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

    fn transfer(opcode: u8, lba: u64, blocks: u16, physical: u64) -> Self {
        let mut command = Self::new(opcode);
        command.0[1] = NAMESPACE_ID;
        command.set_prp(physical);
        command.0[10] = lba as u32;
        command.0[11] = (lba >> 32) as u32;
        command.0[12] = u32::from(blocks - 1);
        command
    }

    #[cfg(feature = "world-session-nvme")]
    fn flush() -> Self {
        let mut command = Self::new(0x00);
        command.0[1] = NAMESPACE_ID;
        command
    }
}

struct CommandCompletion(u16);

impl CommandCompletion {
    fn failed(&self) -> bool {
        self.0 & 0x0ffe != 0
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
    in_flight: Option<u16>,
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
            in_flight: None,
            submission_doorbell: doorbell,
            completion_doorbell: doorbell + stride,
        })
    }

    fn submit(&mut self, registers: Registers, command: Command) -> Result<(), &'static str> {
        self.begin(registers, command)?;
        let completion = poll(IO_TIMEOUT_MILLISECONDS, "COMMAND_TIMEOUT", || {
            self.poll_completion(registers)
        })?;
        if completion.failed() {
            number("COMMAND_STATUS=", u64::from(completion.0 >> 1));
            return Err("COMMAND_STATUS");
        }
        Ok(())
    }

    fn require_idle(&self) -> Result<(), &'static str> {
        if self.in_flight.is_some() {
            return Err("COMMAND_BUSY");
        }
        Ok(())
    }

    fn begin(&mut self, registers: Registers, mut command: Command) -> Result<(), &'static str> {
        self.require_idle()?;
        let cid = self.next_cid;
        self.next_cid = self.next_cid.wrapping_add(1);
        command.0[0] |= u32::from(cid) << 16;
        let entry = usize::from(self.tail) * SUBMISSION_BYTES;
        for (index, word) in command.0.into_iter().enumerate() {
            write_u32(self.submission.base, entry + index * 4, word);
        }
        self.tail = (self.tail + 1) % QUEUE_DEPTH;
        self.in_flight = Some(cid);
        fence(Ordering::SeqCst);
        registers.write(self.submission_doorbell, u32::from(self.tail));
        Ok(())
    }

    fn poll_completion(
        &mut self,
        registers: Registers,
    ) -> Result<Option<CommandCompletion>, &'static str> {
        let cid = self.in_flight.ok_or("COMMAND_NOT_PENDING")?;
        let entry = usize::from(self.head) * COMPLETION_BYTES;
        let footer = self.completion.read_completion_dword(entry + 12);
        let status = (footer >> 16) as u16;
        if (status & 1 != 0) != self.phase {
            if registers.read(CSTS) & CSTS_FATAL != 0 {
                return Err("CONTROLLER_STATUS");
            }
            return Ok(None);
        }

        fence(Ordering::Acquire);
        let route = self.completion.read_completion_dword(entry + 8);
        if footer as u16 != cid || (route >> 16) as u16 != self.id || route as u16 != self.tail {
            return Err("COMPLETION_IDENTITY");
        }
        self.head = (self.head + 1) % QUEUE_DEPTH;
        if self.head == 0 {
            self.phase = !self.phase;
        }
        registers.write(self.completion_doorbell, u32::from(self.head));
        self.in_flight = None;
        Ok(Some(CommandCompletion(status)))
    }
}

fn poll<T>(
    milliseconds: u64,
    timeout: &'static str,
    step: impl FnMut() -> Result<Option<T>, &'static str>,
) -> Result<T, &'static str> {
    #[cfg(feature = "world-session-nvme-target")]
    {
        target::poll(milliseconds, timeout, step)
    }
    #[cfg(not(feature = "world-session-nvme-target"))]
    {
        let _ = milliseconds;
        let mut step = step;
        for _ in 0..4_000_000 {
            if let Some(value) = step()? {
                return Ok(value);
            }
            core::hint::spin_loop();
        }
        Err(timeout)
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
