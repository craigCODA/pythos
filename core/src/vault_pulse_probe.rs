//! Boot and run the one isolated worker selected by ADR 0110.

mod display;

use crate::{
    architecture::x86_64::interrupts,
    block_device,
    memory::{
        physical::PhysicalMemory,
        r#virtual::{
            KernelAddressSpace, KernelAddressSpaceBuildOptions, RetainedUserAddressSpace,
            UserAddressSpace, UserPayloadMapping, with_writable_physical_frame,
        },
    },
    process_context::{self, ActiveUserProcess},
    qemu_exit, runtime_loader, serial,
    service_identity::{ServiceId, ServiceIdentityTable},
    syscall,
    tasks::TaskId,
    user_elf, user_mode, user_stacks,
    vault_pulse_store::{Checkpoint, Delivery, Store},
};
use display::{BootDisplay, BootStage, Origin};
use pythos_shared::{
    boot_protocol::PythBootInfo,
    vault_pulse::{
        Event, EventKind, INVOCATION_LEN, Invocation, OUTCOME_LEN, Outcome, PRINCIPAL, Program,
        Status, arithmetic_failure,
    },
};

const INPUT_ADDRESS: u64 = 0x7300_0000;
const RESULT_ADDRESS: u64 = INPUT_ADDRESS + PAGE_SIZE;
const PAGE_SIZE: u64 = 4096;
const MAX_PULSES_PER_BOOT: usize = 32;

pub(crate) fn run(boot: &PythBootInfo, memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:VAULT:BOOT");
    let mut display = BootDisplay::new(&boot.framebuffer);
    display.stage(BootStage::Core);
    match run_proof(boot, memory, &mut display) {
        Ok(()) => {
            serial::write_line("PYTHOS:VAULT:BOOT_DONE");
            display.hold_terminal();
            qemu_exit::success();
        }
        Err(stage) => {
            let (message, detail) = match stage {
                "VIRTIO_BLOCK" => ("Virtio test disk unavailable", "Restart to exit"),
                "PROOF_DEVICE" | "PROOF_IMAGE" => {
                    ("Marked Virtio test disk required", "Restart to exit")
                }
                _ => ("Vault boot stopped", stage),
            };
            display.stop(stage, message, detail);
            serial::write_str("PYTHOS:VAULT:ERROR:");
            serial::write_line(stage);
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
    }
}

fn run_proof(
    boot: &PythBootInfo,
    memory: &mut PhysicalMemory,
    display: &mut BootDisplay<'_>,
) -> Result<(), &'static str> {
    interrupts::initialize().map_err(|_| "INTERRUPTS")?;
    display.prepare_worker();
    let mut identities = ServiceIdentityTable::new();
    let holder = identities
        .register_task(TaskId::new(1))
        .map_err(|_| "SERVICE_ID")?;
    let worker = Worker::prepare(boot, memory, holder)?;
    display.stage(BootStage::Worker);
    let kernel = KernelAddressSpace::build(memory, boot, KernelAddressSpaceBuildOptions::new())
        .map_err(|_| "KERNEL_ROOT")?;
    // SAFETY: the freshly built kernel root retains all boot, stack, trap,
    // and supervisor scratch mappings needed by this single-core proof.
    unsafe { kernel.activate() };
    kernel.validate_active(boot).map_err(|_| "KERNEL_MAPPING")?;
    syscall::initialize();
    user_stacks::initialize().map_err(|_| "USER_STACKS")?;
    display.stage(BootStage::Memory);
    let device = block_device::select_vault_proof_device().map_err(|_| "VIRTIO_BLOCK")?;
    let mut store = Store::open(device, worker.process.service_id())?;
    display.stage(BootStage::Vault);
    let (mut checkpoint, program, mut pending, origin) = match store.recover()? {
        Some(checkpoint) => {
            serial::write_str("PYTHOS:VAULT:RESTORED");
            write_checkpoint_fields(checkpoint);
            serial::write_line("");
            let Some(program) = store.saved_program(checkpoint)? else {
                serial::write_line("PYTHOS:VAULT:BLOCKED:PROGRAM_REVISION");
                display.stop(
                    "PROGRAM_REVISION",
                    "Saved program unavailable",
                    "Checkpoint retained",
                );
                return Ok(());
            };
            let pending = store
                .receive()?
                .and_then(|delivery| screen_delivery(checkpoint, program, delivery));
            (checkpoint, program, pending, Origin::Restored)
        }
        None => {
            let Some(delivery) = store.receive()? else {
                serial::write_line("PYTHOS:VAULT:IDLE");
                display.stop("IDLE", "Waiting for a task", "Vault is ready");
                return Ok(());
            };
            let program = delivery.program.ok_or("INITIAL_PROGRAM")?;
            if delivery.event.kind != EventKind::Task {
                return Err("INITIAL_TASK");
            }
            let checkpoint = Checkpoint::first(delivery.task, program, delivery.event);
            store.admit_program(program)?;
            store.publish(0, checkpoint)?;
            serial::write_line("PYTHOS:VAULT:ADMITTED");
            (checkpoint, program, None, Origin::Admitted)
        }
    };
    display.program(program, checkpoint, origin);

    for _ in 0..MAX_PULSES_PER_BOOT {
        let incoming = if checkpoint.active.is_none() {
            pending.take()
        } else {
            None
        };
        let working = if let Some(event) = incoming {
            let Some(ready) = prepare_wake(checkpoint, program, event)? else {
                break;
            };
            ready
        } else {
            checkpoint
        };
        let Some(event) = working.active else {
            break;
        };
        let invocation = Invocation {
            generation: working.generation,
            x: working.x,
            cursor: working.cursor,
            program,
            event,
        };
        let Some(outcome) = worker.invoke(&kernel, invocation)? else {
            if let Some(event) = incoming {
                report_event("REJECTED:ARITHMETIC", event.id);
                break;
            }
            serial::write_line("PYTHOS:VAULT:BLOCKED:ARITHMETIC");
            display.stop("ARITHMETIC", "Arithmetic error", "Checkpoint retained");
            return Ok(());
        };
        let candidate = working.advance(outcome)?;
        store.publish(checkpoint.generation, candidate)?;
        checkpoint = candidate;
        report_pulse(checkpoint, outcome.steps);
        display.pulse(checkpoint);
        if let Some(event) = incoming {
            report_event("ACCEPTED", event.id);
        }
        store.after_pulse();
    }
    match checkpoint.status {
        Status::Waiting => {
            let kind = program.wait_kind(checkpoint.cursor).ok_or("WAIT_ADDRESS")?;
            serial::write_str("PYTHOS:VAULT:WAITING");
            field(" kind:", kind as u64);
            serial::write_line("");
        }
        Status::Finished => serial::write_line("PYTHOS:VAULT:FINISHED"),
        Status::Active => {
            serial::write_line("PYTHOS:VAULT:YIELDED");
            display.stop(
                "YIELDED",
                "Pulse budget reached",
                "Checkpoint ready to resume",
            );
        }
    }
    Ok(())
}

fn screen_delivery(checkpoint: Checkpoint, program: Program, delivery: Delivery) -> Option<Event> {
    let event = delivery.event;
    if delivery.task != checkpoint.task
        || delivery
            .program
            .is_some_and(|submitted| submitted != program)
    {
        report_event("CONFLICT", event.id);
        return None;
    }
    if let Some(receipt) = checkpoint.receipt_for(event) {
        if receipt.event == event {
            serial::write_str("PYTHOS:VAULT:DUPLICATE");
            field(" event:", event.id);
            field(" generation:", receipt.generation);
            field(" x:", receipt.x);
            field(" cursor:", u64::from(receipt.cursor));
            serial::write_str(" status:");
            serial::write_str(status_name(receipt.status));
            serial::write_line("");
        } else {
            report_event("CONFLICT", event.id);
        }
        return None;
    }
    Some(event)
}

fn prepare_wake(
    checkpoint: Checkpoint,
    program: Program,
    event: Event,
) -> Result<Option<Checkpoint>, &'static str> {
    if checkpoint.status != Status::Waiting
        || program.wait_kind(checkpoint.cursor) != Some(event.kind)
    {
        report_event("UNMATCHED", event.id);
        return Ok(None);
    }
    match checkpoint.start_event(event) {
        Ok(ready) => Ok(Some(ready)),
        Err("RECEIPT_CAPACITY") => {
            report_event("REJECTED:RECEIPT_CAPACITY", event.id);
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn report_event(status: &str, id: u64) {
    serial::write_str("PYTHOS:VAULT:");
    serial::write_str(status);
    field(" event:", id);
    serial::write_line("");
}

fn report_pulse(checkpoint: Checkpoint, steps: u8) {
    serial::write_str("PYTHOS:VAULT:PULSE");
    write_checkpoint_fields(checkpoint);
    serial::write_str(" status:");
    serial::write_str(status_name(checkpoint.status));
    field(" steps:", u64::from(steps));
    serial::write_line("");
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Active => "active",
        Status::Waiting => "waiting",
        Status::Finished => "finished",
    }
}

fn write_checkpoint_fields(checkpoint: Checkpoint) {
    field(" generation:", checkpoint.generation);
    field(" x:", checkpoint.x);
    field(" cursor:", u64::from(checkpoint.cursor));
}

fn field(label: &str, value: u64) {
    serial::write_str(label);
    serial::write_dec_u64_value(value);
}

struct Worker {
    process: ActiveUserProcess,
    root: RetainedUserAddressSpace,
    entry: u64,
    stack_top: u64,
    input_frame: u64,
    result_frame: u64,
}

impl Worker {
    fn prepare(
        boot: &PythBootInfo,
        memory: &mut PhysicalMemory,
        holder: ServiceId,
    ) -> Result<Self, &'static str> {
        let manifest = runtime_loader::load_named_user_program(boot, b"vault-pulse.elf")
            .map_err(|_| "WORKER_MANIFEST")?;
        if manifest.principal_id() != PRINCIPAL {
            return Err("WORKER_PRINCIPAL");
        }
        let image = user_elf::validate(manifest.elf()).map_err(|_| "WORKER_ELF")?;
        let input_frame = memory.allocate_zeroed_page().map_err(|_| "INPUT_PAGE")?;
        let result_frame = memory.allocate_zeroed_page().map_err(|_| "RESULT_PAGE")?;
        let mappings = [
            UserPayloadMapping::read_only(INPUT_ADDRESS, input_frame, PAGE_SIZE),
            UserPayloadMapping::read_write(RESULT_ADDRESS, result_frame, PAGE_SIZE),
        ];
        let (root, _) = UserAddressSpace::build_with_user_elf_payloads_and_supervisor_mappings(
            memory,
            boot,
            &image,
            manifest.elf(),
            &mappings,
            &[],
        )
        .map_err(|_| "WORKER_ROOT")?;
        root.validate_user_elf_entry(image.entry())
            .map_err(|_| "WORKER_ENTRY")?;
        root.validate_user_payload_mapping(INPUT_ADDRESS, false)
            .map_err(|_| "INPUT_MAPPING")?;
        root.validate_user_payload_mapping(RESULT_ADDRESS, true)
            .map_err(|_| "RESULT_MAPPING")?;
        let stack = user_stacks::regions()[0];
        let process = ActiveUserProcess::from_user_elf_launch(
            holder,
            PRINCIPAL,
            manifest.elf_digest(),
            &image,
            stack,
        )
        .map_err(|_| "WORKER_CONTEXT")?;
        let mut copy_map = process.copy_map();
        copy_map
            .add_mapping(INPUT_ADDRESS, INVOCATION_LEN as u64, true, false)
            .map_err(|_| "INPUT_COPY_MAP")?;
        copy_map
            .add_mapping(RESULT_ADDRESS, OUTCOME_LEN as u64, true, true)
            .map_err(|_| "RESULT_COPY_MAP")?;
        Ok(Self {
            process: process.with_copy_map(copy_map),
            root: root.retain_for_boot(),
            entry: image.entry(),
            stack_top: stack.stack_start + stack.stack_len - 16,
            input_frame,
            result_frame,
        })
    }

    fn invoke(
        &self,
        kernel: &KernelAddressSpace,
        input: Invocation,
    ) -> Result<Option<Outcome>, &'static str> {
        with_writable_physical_frame(self.input_frame, |page| {
            page[..INVOCATION_LEN].copy_from_slice(&input.encode());
        })
        .map_err(|_| "INPUT_WRITE")?;
        with_writable_physical_frame(self.result_frame, |page| page[..OUTCOME_LEN].fill(0))
            .map_err(|_| "RESULT_CLEAR")?;
        // SAFETY: prepare validated this worker's entry, guarded stack, and
        // bounded payload mappings. This is the only active user root.
        unsafe { self.root.activate() };
        let returned = user_mode::run_returnable_user_process(
            self.process,
            self.entry,
            self.stack_top,
            INPUT_ADDRESS,
            RESULT_ADDRESS,
        );
        // SAFETY: the validated kernel root stays live throughout the worker
        // call. The trap path has returned to its retained kernel stack.
        unsafe { kernel.activate() };
        returned.map_err(|_| "WORKER_RETURN")?;
        if process_context::current_caller().is_ok() {
            return Err("WORKER_CONTEXT_LEAK");
        }
        let bytes: [u8; OUTCOME_LEN] = with_writable_physical_frame(self.result_frame, |page| {
            page[..OUTCOME_LEN].try_into().unwrap()
        })
        .map_err(|_| "RESULT_READ")?;
        if bytes == arithmetic_failure(input.generation) {
            return Ok(None);
        }
        let result = Outcome::decode(&bytes).map_err(|_| "RESULT_FORMAT")?;
        result
            .validate_for(input)
            .map_err(|_| "RESULT_VALIDATION")?;
        Ok(Some(result))
    }
}
