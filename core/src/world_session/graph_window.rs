//! Timed scene presentation, title-bar capture, and queued placement.

mod journal;
mod layout;

use core::arch::asm;
use pythos_shared::{
    boot_protocol::PythBootInfo,
    capability_abi::PackedCapability,
    graph_window::{self, GRAPH_NAME, Invocation, PRINCIPAL, Scene, SceneResult, WORKER_NAME},
    pyth_tig::opcode::RESOURCE_OBJECT_WORKSPACE,
    sha256::sha256,
};

use crate::{
    architecture::x86_64::{interrupts, timer},
    block_device::nvme_probe::WorldDisk,
    capabilities::{CapabilityTable, ResourceId, RightsMask},
    framebuffer::graph_window::Presenter,
    memory::{physical::PhysicalMemory, r#virtual::KernelAddressSpace},
    proof_worker::ProofWorker,
    ps2,
    pyth_graph_loader::{self, LoadedPythGraph},
    serial,
    service_identity::ServiceId,
    session_input,
};

use super::{field, signed, with_input_interrupts};
use journal::{Journal, Placement, Progress};
use layout::{Layout, Pose, Update};

pub(super) type Worker =
    ProofWorker<{ graph_window::INVOCATION_LEN }, { graph_window::RESULT_LEN }>;

const DISPLAY_HZ: u32 = 60;
const TICKS_PER_FRAME: u64 = (timer::TIMER_HZ / DISPLAY_HZ) as u64;
const MAX_INPUTS_PER_TURN: usize = 16;

pub(super) fn admit(boot: &PythBootInfo) -> Result<LoadedPythGraph<'_>, &'static str> {
    let loaded = pyth_graph_loader::load_named_pyth_graph(boot, GRAPH_NAME)
        .map_err(|_| "GRAPH_WINDOW_PACKAGE")?;
    if loaded.manifest.principal_id() != PRINCIPAL {
        return Err("GRAPH_WINDOW_PRINCIPAL");
    }
    graph_window::validate_graph(&loaded.verified).map_err(|_| "GRAPH_WINDOW_PROFILE")?;
    Ok(loaded)
}

pub(super) fn prepare(
    boot: &PythBootInfo,
    memory: &mut PhysicalMemory,
    holder: ServiceId,
) -> Result<Worker, &'static str> {
    Worker::prepare(boot, memory, holder, WORKER_NAME, PRINCIPAL)
}

pub(super) fn run(
    boot: &PythBootInfo,
    kernel: &KernelAddressSpace,
    holder: ServiceId,
    worker: &Worker,
    graph: &LoadedPythGraph<'_>,
    disk: &mut WorldDisk,
) -> Result<(), &'static str> {
    let (scene, graph) = execute_graph(graph, kernel, holder, worker)?;
    let mut journal = Journal::recover(disk, graph)?;
    let saved = journal.committed();
    let mut presenter = Presenter::new(&boot.framebuffer, &scene)?;
    let mut layout = Layout::new(
        [
            boot.framebuffer.width as i32,
            boot.framebuffer.height as i32,
        ],
        &scene,
        saved.map(|record| record.drop.pose),
    );
    presenter.draw(&scene, layout.position, layout.cursor, 0)?;

    session_input::bind_session_consumer_quiescent(holder).map_err(|_| "INPUT_BIND")?;
    ps2::initialize().map_err(|_| "PS2")?;
    timer::initialize().map_err(|_| "GRAPH_WINDOW_TIMER")?;
    // SAFETY: the graph worker has returned to the validated kernel root.
    // Only this boot CPU consumes input, polls NVMe and paints. Restoring
    // IF clear keeps the queue check and subsequent STI/HLT atomic. CLI
    // has no pointers or alignment requirements.
    unsafe { asm!("cli", options(nostack)) };
    let origin = timer::ticks();
    let mut last_frame = 0;
    let mut exit_requested = false;
    report_ready(&scene, &layout, saved);

    loop {
        for _ in 0..MAX_INPUTS_PER_TURN {
            if exit_requested {
                break;
            }
            let Some(event) = session_input::try_read_session(holder).map_err(|_| "INPUT_READ")?
            else {
                break;
            };
            match layout.input(event) {
                Update::None => {}
                Update::Captured(sequence) => {
                    serial::write_str("PYTHOS:WORLD:GRAPH_WINDOW_DRAG_BEGIN");
                    field(" sequence:", sequence);
                    serial::write_line("");
                }
                Update::Dropped(drop) => journal.queue(drop),
                Update::Exit => exit_requested = true,
            }
        }
        if exit_requested && !journal.saving() {
            return Ok(());
        }

        let now = timer::ticks();
        let frame = (now - origin) / TICKS_PER_FRAME;
        if frame > last_frame {
            with_input_interrupts(|| {
                presenter.draw(&scene, layout.position, layout.cursor, frame)
            })?;
            report_frame(&scene, &layout, frame, journal.saving())?;
            last_frame = frame;
        }

        // Keep COM1 records intact when the input IRQ first reports activity.
        // NVMe service submits or polls once and returns without waiting.
        match journal.service(now)? {
            Progress::Idle => {}
            Progress::Pending(placement) => {
                report_save("GRAPH_WINDOW_SAVE_PENDING", placement);
                serial::write_line("");
            }
            Progress::BodyReady(placement) => {
                report_save("GRAPH_WINDOW_BODY_READY", placement);
                report_pose(
                    placement.drop.pose,
                    layout.position_for(placement.drop.pose),
                );
                serial::write_line("");
            }
            Progress::Committed(placement) => {
                report_save("GRAPH_WINDOW_COMMITTED", placement);
                report_pose(
                    placement.drop.pose,
                    layout.position_for(placement.drop.pose),
                );
                serial::write_line("");
            }
        }

        // SAFETY: this boot CPU is on the active kernel root with IF clear,
        // owned stack and mapped PIC handlers. PIT and PS/2 can wake it and
        // cannot borrow the scene, NVMe DMA state, or presentation memory.
        unsafe { interrupts::enable_halt_disable() };
    }
}

fn execute_graph(
    loaded: &LoadedPythGraph<'_>,
    kernel: &KernelAddressSpace,
    holder: ServiceId,
    worker: &Worker,
) -> Result<(Scene, [u8; 32]), &'static str> {
    let package = loaded.manifest.package();
    let mut capabilities = CapabilityTable::new();
    let workspace = capabilities
        .grant(
            holder,
            ResourceId::new(u64::from(RESOURCE_OBJECT_WORKSPACE)),
            RightsMask::new(RightsMask::WRITE),
        )
        .map_err(|_| "GRAPH_WINDOW_WORKSPACE")?;
    let workspace = PackedCapability::from_parts(workspace.slot(), workspace.generation());
    let input = Invocation::encode(package, workspace).map_err(|_| "GRAPH_WINDOW_ARGUMENTS")?;
    let output = worker.exchange(kernel, &input)?;
    let result = SceneResult::decode(&output).map_err(|_| "GRAPH_WINDOW_RESULT")?;
    if result.exit.executed_nodes != loaded.verified.package().nodes().len() as u64 {
        return Err("GRAPH_WINDOW_EXECUTION");
    }
    let digest = sha256(package);
    serial::write_str("PYTHOS:WORLD:GRAPH_WINDOW_GRAPH");
    field(" executed:", result.exit.executed_nodes);
    serial::write_str(" sha256:");
    for word in digest.chunks_exact(8) {
        serial::write_hex_u64_value(u64::from_be_bytes(word.try_into().unwrap()));
    }
    serial::write_line("");
    Ok((result.scene, digest))
}

fn report_ready(scene: &Scene, layout: &Layout, saved: Option<Placement>) {
    serial::write_str("PYTHOS:WORLD:GRAPH_WINDOW_READY");
    field(" revision:", saved.map_or(0, |record| record.revision));
    report_pose(
        saved.map_or_else(|| layout.pose(), |record| record.drop.pose),
        layout.position,
    );
    field(" width:", u64::from(scene.width));
    field(" height:", u64::from(scene.height));
    field(
        " title:",
        u64::from(scene.title_bar.y + scene.title_bar.height),
    );
    serial::write_line("");
}

fn report_frame(
    scene: &Scene,
    layout: &Layout,
    frame: u64,
    saving: bool,
) -> Result<(), &'static str> {
    let animation = scene
        .node_rect(usize::from(scene.animation.node_index), frame)
        .ok_or("GRAPH_WINDOW_ANIMATION")?;
    let tick = timer::ticks();
    serial::write_str("PYTHOS:WORLD:GRAPH_WINDOW_FRAME");
    field(" frame:", frame);
    field(" tick:", tick);
    signed(" left:", layout.position[0]);
    signed(" top:", layout.position[1]);
    field(" animation:", u64::from(animation.x));
    field(" drag:", u64::from(layout.dragging()));
    field(" saving:", u64::from(saving));
    serial::write_line("");
    Ok(())
}

fn report_save(stage: &str, placement: Placement) {
    serial::write_str("PYTHOS:WORLD:");
    serial::write_str(stage);
    field(" revision:", placement.revision);
    field(" sequence:", placement.drop.sequence);
}

fn report_pose(pose: Pose, [left, top]: [i32; 2]) {
    signed(" x:", pose.x);
    signed(" y:", pose.y);
    signed(" z:", pose.z);
    signed(" left:", left);
    signed(" top:", top);
}
