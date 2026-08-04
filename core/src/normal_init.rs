//! Normal-boot production initialization (ADR 0052).
//!
//! Extracted from the verification proof paths: each `initialize_*` helper
//! performs only production construction, with no self-test/adversarial
//! markers and no deliberately triggered faults. `physical_memory` and
//! `boot_info` are already constructed by the common prefix in
//! `pythcore_entry` before this module runs.

use crate::block_device::{self, BlockDeviceInfo};
use crate::memory::physical::PhysicalMemory;
use crate::memory::r#virtual::{KernelAddressSpace, RetainedUserAddressSpace, UserAddressSpace};
use crate::{
    architecture, kernel_stacks, pyth_runtime_launch, runtime_loader, serial, syscall, tasks,
    user_elf, user_stacks,
};
use pythos_shared::boot_protocol::PythBootInfo;
use pythos_shared::object_shell_abi::BootstrapCapabilityBlock;

pub const SHELL_BOOTSTRAP_USER_PTR: u64 = 0x0000_0000_7000_0000;

pub struct NormalBootSubstrate {
    /// The active kernel-owned address space normal boot is running under.
    /// Retained (not just its `root_table_phys`) so later normal-boot code can
    /// query or validate it, e.g. via `KernelAddressSpace::validate_active`.
    pub kernel_address_space: KernelAddressSpace,
    pub block_device: BlockDeviceInfo,
    pub shell_launch: PreparedShellLaunch,
    pub pyth_runtime_launch: Option<pyth_runtime_launch::PreparedPythRuntimeLaunch>,
    pub pyth_budget_runtime_launch: Option<pyth_runtime_launch::PreparedPythRuntimeLaunch>,
    pub pyth_invalid_graph_rejection: Option<pyth_runtime_launch::PythGraphRejectCode>,
}

pub struct PreparedShellLaunch {
    pub address_space: RetainedUserAddressSpace,
    pub image: user_elf::UserElfImage,
    pub entry: u64,
    pub principal_id: u64,
    pub program_digest: u64,
    pub bootstrap_user_ptr: u64,
    bootstrap_kernel_ptr: u64,
    pub stack_region: user_stacks::UserStackRegion,
}

impl PreparedShellLaunch {
    pub const fn bootstrap_kernel_ptr(&self) -> u64 {
        self.bootstrap_kernel_ptr
    }

    pub const fn user_stack_top(&self) -> u64 {
        self.stack_region.stack_start + self.stack_region.stack_len - 16
    }

    pub fn write_bootstrap_block(&self, block: &BootstrapCapabilityBlock) {
        // SAFETY:
        // 1. Invariant: `bootstrap_kernel_ptr` is the supervisor-writable
        //    mapping of the shell bootstrap frame in the active kernel root.
        // 2. Established by: `KernelAddressSpace::build` maps the allocated
        //    bootstrap frame before normal boot activates that root.
        // 3. Lifetime: the frame is retained for the shell process lifetime.
        // 4. Pointer ownership: PythCore exclusively initializes this block
        //    before entering the shell; the shell sees only a read-only alias.
        // 5. Alignment: the frame is 4 KiB aligned and the block is 8-byte aligned.
        // 6. Mapped length: one 4 KiB frame contains the 168-byte bootstrap block.
        // 7. Concurrency: single-core normal boot, before user shell execution.
        // 8. Violation: a bad mapping faults before `PYTHOS:SHELL:RING3_ENTER`.
        unsafe {
            (self.bootstrap_kernel_ptr as *mut BootstrapCapabilityBlock).write(*block);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalInitError {
    Memory,
    InterruptsTimer,
    TaskProcess,
    Ring3,
    UserStacks,
    BlockDevice,
    ShellProgram,
    ShellAddressSpace,
    ShellBootstrap,
}

#[cfg(not(test))]
pub fn initialize_normal_substrate(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
) -> Result<NormalBootSubstrate, NormalInitError> {
    // Build every address space that needs `PageTableBuilder`'s raw-physical-
    // address table writes *before* activating the kernel address space. Once
    // `KernelAddressSpace::activate()` switches CR3, the loader's broad low-
    // memory identity map is gone by design (the Phase 1.5 identity-map-removal
    // invariant), so a later `UserAddressSpace::build()` would fault writing
    // its own fresh page-table frames. The verify path relies on the same
    // ordering (see `pythcore_entry`): build kernel + user address spaces,
    // then activate only the kernel one.
    let shell_manifest = runtime_loader::load_named_user_program(boot_info, b"shell.elf")
        .map_err(|_| NormalInitError::ShellProgram)?;
    let shell_image =
        user_elf::validate(shell_manifest.elf()).map_err(|_| NormalInitError::ShellProgram)?;
    let bootstrap_frame = physical_memory
        .allocate_zeroed_page()
        .map_err(|_| NormalInitError::ShellBootstrap)?;
    let ahci_controller = block_device::probe_ahci();
    let ahci_mmio = ahci_controller.map(|c| {
        (
            c.mmio_base,
            block_device::AHCI_MMIO_VIRT,
            block_device::AHCI_MMIO_LEN,
        )
    });
    #[cfg(feature = "sdhci-emmc-backend")]
    let sdhci_emmc_controller = match crate::sdhci_emmc::probe_controller() {
        Ok(controller) => Some(controller),
        Err(crate::sdhci_emmc::SdhciEmmcBackendError::DeviceAbsent) => None,
        Err(_) => return Err(NormalInitError::BlockDevice),
    };
    #[cfg(feature = "sdhci-emmc-backend")]
    let sdhci_emmc_mmio = sdhci_emmc_controller.map(|controller| {
        (
            controller.physical_mmio_base,
            crate::sdhci_emmc::SDHCI_EMMC_MMIO_VIRT,
            crate::sdhci_emmc::SDHCI_EMMC_MMIO_LEN,
        )
    });
    #[cfg(not(feature = "sdhci-emmc-backend"))]
    let sdhci_emmc_mmio = None;
    let kernel_address_space = KernelAddressSpace::build(
        physical_memory,
        boot_info,
        None,
        ahci_mmio,
        sdhci_emmc_mmio,
        Some(bootstrap_frame),
    )
    .map_err(|_| NormalInitError::Memory)?;
    let supervisor_mappings = [ahci_mmio, sdhci_emmc_mmio];
    let (shell_address_space, loaded_shell) =
        UserAddressSpace::build_with_user_elf_bootstrap_and_supervisor_mappings(
            physical_memory,
            boot_info,
            &shell_image,
            shell_manifest.elf(),
            SHELL_BOOTSTRAP_USER_PTR,
            bootstrap_frame,
            &supervisor_mappings,
        )
        .map_err(|_| NormalInitError::ShellAddressSpace)?;
    if loaded_shell.entry() != shell_image.entry()
        || loaded_shell.segment_count() != shell_image.segment_count()
        || !loaded_shell.bss_zeroed()
    {
        return Err(NormalInitError::ShellAddressSpace);
    }
    shell_address_space
        .validate_user_elf_entry(shell_image.entry())
        .map_err(|_| NormalInitError::ShellAddressSpace)?;
    shell_address_space
        .validate_user_bootstrap_mapping(SHELL_BOOTSTRAP_USER_PTR)
        .map_err(|_| NormalInitError::ShellBootstrap)?;
    let mut mapping_index = 0;
    while mapping_index < supervisor_mappings.len() {
        if let Some((phys, virt, _)) = supervisor_mappings[mapping_index] {
            shell_address_space
                .validate_supervisor_mapping(virt, phys)
                .map_err(|_| NormalInitError::ShellAddressSpace)?;
        }
        mapping_index += 1;
    }
    let shell_launch = PreparedShellLaunch {
        address_space: shell_address_space.retain_for_boot(),
        image: shell_image,
        entry: shell_image.entry(),
        principal_id: shell_manifest.principal_id(),
        program_digest: shell_manifest.elf_digest(),
        bootstrap_user_ptr: SHELL_BOOTSTRAP_USER_PTR,
        bootstrap_kernel_ptr: bootstrap_frame,
        stack_region: user_stacks::regions()[0],
    };
    let pyth_runtime_launch = pyth_runtime_launch::prepare_pyth_runtime_launch(
        boot_info,
        physical_memory,
        &supervisor_mappings,
    )
    .ok();
    let pyth_budget_runtime_launch = pyth_runtime_launch::prepare_pyth_runtime_launch_for_graph(
        boot_info,
        physical_memory,
        &supervisor_mappings,
        pyth_runtime_launch::BUDGET_GRAPH_NAME,
        pyth_runtime_launch::BUDGET_GRAPH_PRINCIPAL_ID,
    )
    .ok();
    let pyth_invalid_graph_rejection = pyth_runtime_launch::detect_pyth_graph_rejection(
        boot_info,
        pyth_runtime_launch::INVALID_GRAPH_NAME,
    );
    // SAFETY:
    // 1. Invariant: `kernel_address_space` maps the currently executing
    //    PythCore code, active bootstrap stack, boot metadata, and
    //    framebuffer.
    // 2. Established by: successful `KernelAddressSpace::build` immediately
    //    above, mirroring the verification path's identical activation.
    // 3. Lifetime: the page tables are retained for the life of normal boot.
    // 4. Pointer ownership: PythCore owns the newly allocated page tables.
    // 5. Alignment: the table root was allocated as a 4 KiB physical page.
    // 6. Mapped length: the full active early-core address surface is mapped.
    // 7. Concurrency: single-core execution with interrupts disabled.
    // 8. Violation: a broken mapping faults immediately after the CR3 switch.
    unsafe {
        kernel_address_space.activate();
    }
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:MEMORY_VM_READY");
    #[cfg(feature = "sdhci-emmc-backend")]
    let sdhci_emmc_device = match sdhci_emmc_controller {
        Some(controller) => Some(
            crate::sdhci_emmc::initialize_device(controller)
                .map_err(|_| NormalInitError::BlockDevice)?,
        ),
        None => None,
    };
    // GDT/TSS ring-3 selectors are already installed by the common
    // `gdt::initialize()` call before the verify/normal branch; this marker
    // covers the user address-space construction proved just above.
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:RING3_READY");

    initialize_interrupts_timer_and_clock().map_err(|_| NormalInitError::InterruptsTimer)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:INTERRUPTS_TIMER_READY");

    initialize_task_process_and_kernel_stack_state(boot_info)
        .map_err(|_| NormalInitError::TaskProcess)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:TASK_PROCESS_READY");

    syscall::initialize();
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:SYSCALL_READY");

    initialize_guarded_user_stack_pool().map_err(|_| NormalInitError::UserStacks)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:USER_STACKS_READY");

    #[cfg(feature = "sdhci-emmc-backend")]
    let block_device_selection = block_device::select_device_with_sdhci_emmc(sdhci_emmc_device);
    #[cfg(not(feature = "sdhci-emmc-backend"))]
    let block_device_selection = block_device::select_device();
    let block_device = block_device_selection.map_err(|_| NormalInitError::BlockDevice)?;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY");

    Ok(NormalBootSubstrate {
        kernel_address_space,
        block_device,
        shell_launch,
        pyth_runtime_launch,
        pyth_budget_runtime_launch,
        pyth_invalid_graph_rejection,
    })
}

#[cfg(not(test))]
fn initialize_interrupts_timer_and_clock() -> Result<(), NormalInitError> {
    architecture::x86_64::interrupts::initialize().map_err(|_| NormalInitError::InterruptsTimer)?;
    architecture::x86_64::timer::initialize().map_err(|_| NormalInitError::InterruptsTimer)?;
    architecture::x86_64::clock::initialize().map_err(|_| NormalInitError::InterruptsTimer)?;
    Ok(())
}

#[cfg(not(test))]
fn initialize_task_process_and_kernel_stack_state(
    boot_info: &'static PythBootInfo,
) -> Result<(), NormalInitError> {
    tasks::initialize(boot_info).map_err(|_| NormalInitError::TaskProcess)?;
    kernel_stacks::initialize(boot_info).map_err(|_| NormalInitError::TaskProcess)?;
    Ok(())
}

#[cfg(not(test))]
fn initialize_guarded_user_stack_pool() -> Result<(), NormalInitError> {
    user_stacks::initialize().map_err(|_| NormalInitError::UserStacks)
}
