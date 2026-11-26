//! Native world input, returnable world worker, and committed presentation.

#[cfg(feature = "world-session-graph-window")]
mod graph_window;
#[cfg(feature = "world-session-nvme-usb")]
mod keyboard;
#[cfg(not(feature = "world-session-graph-window"))]
mod store;

#[cfg(all(
    feature = "world-session-graph-window",
    feature = "world-session-nvme-target"
))]
compile_error!("the graph window proof uses the emulated NVMe fixture");

use core::arch::asm;

#[cfg(not(feature = "world-session-nvme"))]
use crate::block_device;
#[cfg(feature = "world-session-nvme")]
use crate::block_device::nvme_probe::NvmeLocation;
#[cfg(not(any(
    feature = "world-session-nvme-usb",
    feature = "world-session-graph-window"
)))]
use crate::ps2;
#[cfg(not(feature = "world-session-graph-window"))]
use crate::{architecture::x86_64::interrupts, proof_worker::ProofWorker, session_input};
use crate::{
    framebuffer,
    memory::{
        physical::PhysicalMemory,
        r#virtual::{KernelAddressSpace, KernelAddressSpaceBuildOptions},
    },
    qemu_exit, serial,
    service_identity::{ServiceId, ServiceIdentityTable},
    syscall,
    tasks::TaskId,
    user_stacks,
};
use pythos_shared::boot_protocol::PythBootInfo;
#[cfg(not(feature = "world-session-graph-window"))]
use pythos_shared::{
    session_input_abi::{KEY_DIGIT0, SESSION_INPUT_KIND_KEY_DOWN},
    world_session::{FRAME_LEN, Frame, INVOCATION_LEN, Invocation, Notice, PRINCIPAL, World},
};

pub(crate) fn run(boot: &PythBootInfo, memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:WORLD:BOOT");
    #[cfg(feature = "world-session-nvme-target")]
    let _ = framebuffer::render_hardware_probe_lines(
        &boot.framebuffer,
        &[
            "PythOS NVMe world",
            "Checking controller and reserved partition",
        ],
    );
    let result = run_session(boot, memory);
    #[cfg(feature = "world-session-nvme-target")]
    let result = if result == Err("NVME_TARGET_HEADER_REQUIRED") {
        framebuffer::render_hardware_probe_lines(
            &boot.framebuffer,
            &[
                "PythOS NVMe target verified",
                "Controller and reserved partition matched",
                "World header required",
            ],
        )
        .map_err(|_| "NVME_TARGET_DISPLAY")
        .map(|_| serial::write_line("PYTHOS:WORLD:NVME_TARGET_VERIFIED"))
    } else {
        result
    };
    match result {
        Ok(()) => {
            serial::write_line("PYTHOS:WORLD:EXIT");
            finish(true);
        }
        Err(stage) => {
            let detail = match stage {
                #[cfg(not(feature = "world-session-nvme"))]
                "WORLD_DEVICE" | "VIRTIO" => "Attach the native world Virtio image",
                #[cfg(feature = "world-session-nvme")]
                "WORLD_DEVICE" => "The NVMe partition is too small for world checkpoints",
                #[cfg(not(feature = "world-session-nvme-target"))]
                "CONTROLLER_ABSENT" => "Attach the native world NVMe test image",
                #[cfg(feature = "world-session-nvme-target")]
                "CONTROLLER_ABSENT" => "Configured NVMe controller unavailable",
                "WORLD_IMAGE" => "The attached disk is not a marked world image",
                _ => stage,
            };
            let _ = framebuffer::render_hardware_probe_lines(
                &boot.framebuffer,
                &["PythOS native world stopped", detail],
            );
            serial::write_str("PYTHOS:WORLD:ERROR:");
            serial::write_line(stage);
            serial::write_line("PYTHOS:PANIC");
            finish(false);
        }
    }
}

fn finish(success: bool) -> ! {
    #[cfg(feature = "world-session-nvme-target")]
    if !crate::block_device::nvme_probe::EMULATED_TARGET {
        loop {
            // SAFETY: the boot CPU is at the terminal world screen. The
            // controller teardown ran before this call and no worker is
            // active. CLI and HLT need no pointer, alignment, or mapped
            // span. IRQ handling past this point could touch stopped state.
            unsafe { asm!("cli", "hlt", options(nomem, nostack)) };
        }
    }
    if success {
        qemu_exit::success();
    }
    qemu_exit::panic();
}

fn run_session(boot: &PythBootInfo, memory: &mut PhysicalMemory) -> Result<(), &'static str> {
    crate::architecture::x86_64::interrupts::initialize().map_err(|_| "INTERRUPTS")?;
    let mut identities = ServiceIdentityTable::new();
    let holder = identities
        .register_task(TaskId::new(1))
        .map_err(|_| "WORLD_IDENTITY")?;
    #[cfg(feature = "world-session-graph-window")]
    let graph = graph_window::admit(boot)?;
    #[cfg(feature = "world-session-graph-window")]
    let worker = graph_window::prepare(boot, memory, holder)?;
    #[cfg(not(feature = "world-session-graph-window"))]
    let worker = ProofWorker::<INVOCATION_LEN, FRAME_LEN>::prepare(
        boot,
        memory,
        holder,
        b"world-session.elf",
        PRINCIPAL,
    )?;
    let options = KernelAddressSpaceBuildOptions::new();
    #[cfg(feature = "world-session-nvme")]
    let nvme = NvmeLocation::discover()?;
    #[cfg(feature = "world-session-nvme")]
    let options = KernelAddressSpaceBuildOptions {
        nvme_mmio: Some(nvme.mapping()),
        ..options
    };
    #[cfg(feature = "world-session-nvme-usb")]
    let usb = keyboard::UsbLocation::discover()?;
    #[cfg(feature = "world-session-nvme-usb")]
    let options = KernelAddressSpaceBuildOptions {
        xhci_mmio: Some(usb.mapping()),
        ..options
    };
    let kernel = KernelAddressSpace::build(memory, boot, options).map_err(|_| "KERNEL_ROOT")?;
    // SAFETY: the new validated kernel root owns the current stack and retains
    // the interrupt, framebuffer and device mappings for this single CPU's
    // boot lifetime. No worker or input producer is active during the switch.
    unsafe { kernel.activate() };
    kernel.validate_active(boot).map_err(|_| "KERNEL_MAPPING")?;
    syscall::initialize();
    user_stacks::initialize().map_err(|_| "USER_STACKS")?;
    #[cfg(not(feature = "world-session-nvme"))]
    {
        let device = block_device::select_vault_proof_device().map_err(|_| "VIRTIO")?;
        run_world(
            boot,
            &kernel,
            holder,
            &worker,
            store::Store::open(device, worker.service_id())?,
        )
    }
    #[cfg(feature = "world-session-graph-window")]
    {
        nvme.with_world_disk(|disk| {
            let result = graph_window::run(boot, &kernel, holder, &worker, &graph, disk);
            disk.cancel_record();
            result
        })
    }
    #[cfg(all(
        feature = "world-session-nvme",
        not(feature = "world-session-graph-window")
    ))]
    {
        nvme.with_world_disk(|device| {
            let store = store::Store::open(device, worker.service_id())?;
            #[cfg(not(feature = "world-session-nvme-usb"))]
            {
                run_world(boot, &kernel, holder, &worker, store)
            }
            #[cfg(feature = "world-session-nvme-usb")]
            {
                usb.with_keyboard(|keyboard| {
                    run_world(boot, &kernel, holder, &worker, store, keyboard)
                })
            }
        })
    }
}

#[cfg(not(feature = "world-session-graph-window"))]
fn run_world(
    boot: &PythBootInfo,
    kernel: &KernelAddressSpace,
    holder: ServiceId,
    worker: &ProofWorker<INVOCATION_LEN, FRAME_LEN>,
    mut store: store::Store,
    #[cfg(feature = "world-session-nvme-usb")] keyboard: &mut keyboard::WorldKeyboard,
) -> Result<(), &'static str> {
    let saved = store.recover()?;
    let mut frame = Frame::restored(saved.unwrap_or_else(World::blank));
    session_input::bind_session_consumer_quiescent(holder).map_err(|_| "INPUT_BIND")?;
    #[cfg(not(feature = "world-session-nvme-usb"))]
    ps2::initialize().map_err(|_| "PS2")?;
    present(boot, frame)?;
    if saved.is_some() {
        report("RESTORED", frame);
    }
    report("READY", frame);
    loop {
        #[cfg(feature = "world-session-nvme-usb")]
        keyboard.poll()?;
        let event = match session_input::try_read_session(holder).map_err(|_| "INPUT_READ")? {
            Some(event) => event,
            None => {
                // SAFETY: this single bound consumer is on its live kernel
                // stack with IF clear and the input or timer IRQ mapped.
                // STI/HLT/CLI closes the legacy queue sleep race. The timer
                // also wakes the USB profile for its next bounded poll.
                unsafe { interrupts::enable_halt_disable() };
                continue;
            }
        };
        serial::write_str("PYTHOS:WORLD:INPUT");
        field(" sequence:", event.sequence);
        field(" kind:", u64::from(event.kind));
        signed(" value:", event.value0);
        field(" flags:", u64::from(event.flags));
        serial::write_line("");
        let input = Invocation { frame, event };
        let reply = worker.exchange(kernel, &input.encode())?;
        let next = Frame::decode(&reply)
            .and_then(|candidate| candidate.validate_result(input))
            .map_err(|_| "WORLD_RESULT")?;
        if next.view.notice == Notice::Exit {
            if event.kind != SESSION_INPUT_KIND_KEY_DOWN
                || event.value0 != i32::from(KEY_DIGIT0)
                || next.world != frame.world
            {
                return Err("WORLD_EXIT");
            }
            present(boot, next)?;
            return Ok(());
        }
        if next.world.revision != frame.world.revision {
            with_input_interrupts(|| store.publish(frame.world.revision, next.world))?;
            report_edit(next.world);
        }
        if next != frame {
            present(boot, next)?;
            report("FRAME", next);
            frame = next;
        }
    }
}

#[cfg(not(feature = "world-session-graph-window"))]
fn present(boot: &PythBootInfo, frame: Frame) -> Result<(), &'static str> {
    with_input_interrupts(|| framebuffer::world_scene::render(&boot.framebuffer, &frame))
        .inspect_err(|_| {
            report("RENDER_CANDIDATE", frame);
            serial::write_line(frame.view.notice.label());
        })
}

fn with_input_interrupts<T>(work: impl FnOnce() -> T) -> T {
    // SAFETY: the native session calls this on its retained kernel root
    // with IF clear. USB setup allows the initialized PIT handler. Legacy
    // world presentation and storage also allow the bound PS/2 producers.
    // Their handlers cannot borrow the renderer, store, or USB driver,
    // switch roots, or reenter this consumer. STI has no pointer, alignment,
    // or mapped-span requirements. Calling it in another context could
    // expose incomplete mappings or state to an interrupt.
    unsafe { asm!("sti", options(nostack)) };
    let result = work();
    // SAFETY: restore the caller's IF-clear invariant before it touches the
    // worker root or rechecks the queue. This instruction has no pointer,
    // alignment, or length requirements and runs on the same mapped CPU.
    unsafe { asm!("cli", options(nostack)) };
    result
}

#[cfg(not(feature = "world-session-graph-window"))]
fn report(stage: &str, frame: Frame) {
    serial::write_str("PYTHOS:WORLD:");
    serial::write_str(stage);
    field(" revision:", frame.world.revision);
    field(" objects:", frame.world.objects().count() as u64);
    signed(" x:", frame.view.pose.x);
    signed(" z:", frame.view.pose.z);
    field(" heading:", u64::from(frame.view.pose.heading));
    field(" selected:", u64::from(frame.view.selected));
    field(" target:", u64::from(frame.view.target));
    field(" focus:", u64::from(frame.world.focus));
    serial::write_line("");
}

#[cfg(not(feature = "world-session-graph-window"))]
fn report_edit(world: World) {
    serial::write_str("PYTHOS:WORLD:EDIT");
    field(" revision:", world.revision);
    field(" sequence:", world.receipt.sequence);
    serial::write_str(" operation:");
    serial::write_str(world.receipt.edit.label());
    field(" actor:", u64::from(world.receipt.actor));
    field(" target:", u64::from(world.receipt.target));
    serial::write_line("");
}

fn field(label: &str, value: u64) {
    serial::write_str(label);
    serial::write_dec_u64_value(value);
}

fn signed(label: &str, value: i32) {
    serial::write_str(label);
    if value < 0 {
        serial::write_str("-");
    }
    serial::write_dec_u64_value(u64::from(value.unsigned_abs()));
}
