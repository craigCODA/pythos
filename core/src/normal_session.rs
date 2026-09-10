//! One retained normal launch and one-way recovery (ADR 0093).

use crate::user_mode::{UserFaultContext, UserModeError};
use pythos_shared::normal_session_abi::*;

fn build_bootstrap(
    mut graph: pythos_shared::pyth_runtime_abi::PythGraphBootstrapBlock,
    digest: u64,
    capabilities: [pythos_shared::capability_abi::PackedCapability; 3],
) -> Result<NormalSessionBootstrapV1, &'static str> {
    let mut bootstrap = NormalSessionBootstrapV1::empty();
    // The existing graph binding helper supplies the compatibility budget;
    // this launch owns ADR 0093's exact production instruction limit.
    graph.instruction_budget = NORMAL_SESSION_GRAPH_INSTRUCTION_BUDGET;
    bootstrap.magic = NORMAL_SESSION_BOOTSTRAP_MAGIC;
    bootstrap.abi_major = NORMAL_SESSION_ABI_MAJOR;
    bootstrap.abi_minor = NORMAL_SESSION_ABI_MINOR;
    bootstrap.session_service_id = NORMAL_SESSION_SERVICE_ID;
    bootstrap.runtime_principal_id =
        pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID;
    bootstrap.graph_principal_id = NORMAL_SESSION_GRAPH_PRINCIPAL_ID;
    [
        bootstrap.console_capability,
        bootstrap.input_capability,
        bootstrap.presentation_capability,
    ] = capabilities;
    bootstrap.graph_package_digest = digest;
    bootstrap.return_ptr = NORMAL_SESSION_RETURN_ADDRESS;
    bootstrap.return_len = core::mem::size_of::<NormalSessionReturnV1>() as u64;
    bootstrap.width = NORMAL_SESSION_VIEWPORT_WIDTH;
    bootstrap.height = NORMAL_SESSION_VIEWPORT_HEIGHT;
    bootstrap.graph = graph;
    validate_normal_session_bootstrap(&bootstrap).map_err(|_| "bootstrap")?;
    Ok(bootstrap)
}

#[cfg(not(test))]
pub(crate) struct PreparedNormalSession {
    pub(crate) address_space: crate::memory::r#virtual::UserAddressSpace,
    process: crate::process_context::ActiveUserProcess,
    entry: u64,
    stack: crate::user_stacks::UserStackRegion,
    graph: crate::pyth_graph_loader::LoadedPythGraph<'static>,
    bootstrap_frame: u64,
    return_frame: u64,
}

#[cfg(not(test))]
pub(crate) fn prepare(
    boot_info: &'static pythos_shared::boot_protocol::PythBootInfo,
    memory: &mut crate::memory::physical::PhysicalMemory,
) -> Result<PreparedNormalSession, &'static str> {
    use crate::{
        memory::r#virtual::{UserAddressSpace, UserPayloadMapping},
        process_context::{ActiveUserProcess, PythRuntimeCopyMapSpec},
        service_identity::ServiceId,
    };
    let program = crate::runtime_loader::load_named_user_program(
        boot_info,
        pythos_shared::user_program_manifest::NORMAL_SESSION_PROGRAM_NAME,
    )
    .map_err(|_| "program-admission")?;
    let image = crate::user_elf::validate(program.elf()).map_err(|_| "elf")?;
    let graph = crate::pyth_graph_loader::load_named_pyth_graph(boot_info, b"session-manager.tig")
        .map_err(|_| "graph-admission")?;
    let length = graph.manifest.package().len() as u64;
    if graph.manifest.principal_id() != NORMAL_SESSION_GRAPH_PRINCIPAL_ID
        || length == 0
        || length > NORMAL_SESSION_PAGE_SIZE
    {
        return Err("graph-identity-range");
    }
    // Only ELF and payload frames occupy this ledger; static stack pages and
    // page-table frames have separate owners. Check actual rounded PT_LOADs.
    let mut retained_pages = 3usize;
    for index in 0..image.segment_count() {
        retained_pages = retained_pages
            .checked_add(
                (image.segment(index).ok_or("elf")?.page_len() / NORMAL_SESSION_PAGE_SIZE) as usize,
            )
            .ok_or("frame-capacity")?;
    }
    if retained_pages > crate::memory::r#virtual::MAX_RETAINED_USER_FRAMES {
        return Err("frame-capacity");
    }
    let stack = crate::user_stacks::regions()[0];
    let process = ActiveUserProcess::from_pyth_native_launch(
        ServiceId::from_raw(NORMAL_SESSION_SERVICE_ID),
        program.principal_id(),
        program.elf_digest(),
        &image,
        PythRuntimeCopyMapSpec {
            stack,
            bootstrap_user_ptr: NORMAL_SESSION_BOOTSTRAP_ADDRESS,
            bootstrap_len: NORMAL_SESSION_PAGE_SIZE,
            package_user_ptr: NORMAL_SESSION_GRAPH_PACKAGE_ADDRESS,
            package_len: NORMAL_SESSION_PAGE_SIZE,
            result_user_ptr: NORMAL_SESSION_RETURN_ADDRESS,
            result_len: NORMAL_SESSION_PAGE_SIZE,
        },
    )
    .map_err(|_| "copy-map")?;
    let bootstrap_frame = memory
        .allocate_zeroed_page()
        .map_err(|_| "bootstrap-frame")?;
    let package_frame = memory.allocate_zeroed_page().map_err(|_| "package-frame")?;
    let return_frame = memory.allocate_zeroed_page().map_err(|_| "return-frame")?;
    crate::memory::r#virtual::with_writable_physical_frame(package_frame, |page| {
        page[..length as usize].copy_from_slice(graph.manifest.package());
    })
    .map_err(|_| "package-copy")?;
    let payloads = [
        UserPayloadMapping::read_only(
            NORMAL_SESSION_BOOTSTRAP_ADDRESS,
            bootstrap_frame,
            NORMAL_SESSION_PAGE_SIZE,
        ),
        UserPayloadMapping::read_only(
            NORMAL_SESSION_GRAPH_PACKAGE_ADDRESS,
            package_frame,
            NORMAL_SESSION_PAGE_SIZE,
        ),
        UserPayloadMapping::read_write(
            NORMAL_SESSION_RETURN_ADDRESS,
            return_frame,
            NORMAL_SESSION_PAGE_SIZE,
        ),
    ];
    let fb = boot_info.framebuffer;
    let (address_space, loaded) =
        UserAddressSpace::build_with_user_elf_payloads_and_selected_stack(
            memory,
            boot_info,
            &image,
            program.elf(),
            &payloads,
            &[Some((
                fb.physical_base,
                fb.mapped_virtual_base,
                fb.byte_length,
            ))],
            0,
        )
        .map_err(|_| "session-root")?;
    if loaded.entry() != image.entry()
        || loaded.segment_count() != image.segment_count()
        || !loaded.bss_zeroed()
    {
        return Err("elf-copy");
    }
    address_space
        .validate_user_elf_entry_with_selected_stack(image.entry(), 0)
        .map_err(|_| "entry-mapping")?;
    for mapping in payloads {
        address_space
            .validate_user_payload_mapping(mapping.user_ptr, mapping.writable)
            .map_err(|_| "payload-mapping")?;
    }
    for offset in [0, fb.byte_length.checked_sub(1).ok_or("framebuffer-range")?] {
        address_space
            .validate_supervisor_writable_nx_mapping(
                fb.mapped_virtual_base
                    .checked_add(offset)
                    .ok_or("framebuffer-range")?,
                fb.physical_base
                    .checked_add(offset)
                    .ok_or("framebuffer-range")?,
            )
            .map_err(|_| "framebuffer-mapping")?;
    }
    Ok(PreparedNormalSession {
        address_space,
        process,
        entry: image.entry(),
        stack,
        graph,
        bootstrap_frame,
        return_frame,
    })
}

#[cfg(not(test))]
struct KernelRecovery<'a> {
    boot_info: &'static pythos_shared::boot_protocol::PythBootInfo,
    root: &'a crate::memory::r#virtual::KernelAddressSpace,
    grants: Option<crate::syscall::NormalSessionGrants>,
    presenter_bound: bool,
    queue_bound: bool,
}

#[cfg(not(test))]
impl RecoveryBoundary for KernelRecovery<'_> {
    fn restore_kernel(&mut self) -> bool {
        // SAFETY: both roots were built before activation. The retained kernel
        // root maps this continuation, its stack, trap code and metadata. The
        // single CPU has returned from user code; no resources are reclaimed.
        unsafe {
            self.root.activate();
        }
        self.root.validate_active(self.boot_info).is_ok()
    }
    fn caller_cleared(&mut self) -> bool {
        crate::process_context::current_caller().is_err()
            && crate::user_mode::returnable_transients_cleared()
    }
    fn revoke_grants(&mut self) -> bool {
        self.grants.as_mut().is_none_or(|grants| {
            crate::syscall::revoke_normal_session_capabilities(grants).is_ok()
                && grants.is_revoked()
        })
    }
    fn disable_presenter(&mut self) -> bool {
        !self.presenter_bound
            || crate::session_presentation::disable(crate::service_identity::ServiceId::from_raw(
                NORMAL_SESSION_SERVICE_ID,
            ))
            .is_ok()
    }
    fn queue_retained(&mut self) -> bool {
        let ready = crate::session_input::session_ready(
            crate::service_identity::ServiceId::from_raw(NORMAL_SESSION_SERVICE_ID),
        );
        if self.queue_bound {
            ready.is_ok()
        } else {
            ready == Err(crate::session_input::SessionInputError::SessionUnbound)
        }
    }
}

#[cfg(not(test))]
pub(crate) fn run(
    boot_info: &'static pythos_shared::boot_protocol::PythBootInfo,
    substrate: &crate::normal_init::NormalBootSubstrate,
) -> ! {
    use crate::{serial, syscall, user_mode};
    serial::init_com2();
    serial::write_line("PYTHOS:CORE:COM2_READY");
    let mut supervisor = NormalSessionSupervisor::new();
    let mut boundary = KernelRecovery {
        boot_info,
        root: &substrate.kernel_address_space,
        grants: None,
        presenter_bound: false,
        queue_bound: false,
    };
    let launch = (|| {
        let prepared = substrate.normal_session.as_ref().map_err(|error| *error)?;
        let grants = syscall::grant_normal_session_capabilities(prepared.process)
            .map_err(|_| "grant-bind")?;
        boundary.queue_bound = true;
        boundary.grants = Some(grants);
        let grants = boundary.grants.as_ref().ok_or("grants")?;
        let graph = crate::pyth_runtime_launch::build_pyth_command_graph_bootstrap(
            &prepared.graph.verified,
            NORMAL_SESSION_GRAPH_PACKAGE_ADDRESS,
            prepared.graph.manifest.package().len() as u64,
            NORMAL_SESSION_GRAPH_RESULT_ADDRESS,
            grants.command(),
        )
        .map_err(|_| "graph-bootstrap")?;
        let bootstrap = build_bootstrap(
            graph,
            prepared.graph.manifest.package_digest(),
            [grants.console(), grants.input(), grants.presentation()],
        )?;
        crate::memory::r#virtual::with_writable_physical_frame(prepared.bootstrap_frame, |page| {
            // SAFETY: scratch mapping exclusively borrows a retained 4 KiB
            // frame; this aligned 944-byte Copy record fits and is written
            // before entry. User bootstrap alias is read-only and no IRQ reads it.
            unsafe {
                page.as_mut_ptr()
                    .cast::<NormalSessionBootstrapV1>()
                    .write(bootstrap);
            }
        })
        .map_err(|_| "bootstrap-write")?;
        let extent = pythos_shared::viewing::ViewingExtent::new(
            NORMAL_SESSION_VIEWPORT_WIDTH,
            NORMAL_SESSION_VIEWPORT_HEIGHT,
        )
        .map_err(|_| "viewport")?;
        // SAFETY: prepare validated the whole framebuffer in both retained
        // roots. This single CPU owns its only presenter; binding precedes
        // IRQ publication and user entry, and all mappings live until reboot.
        unsafe {
            crate::session_presentation::bind(
                prepared.process.service_id(),
                boot_info.framebuffer,
                extent,
            )
        }
        .map_err(|_| "presenter-bind")?;
        boundary.presenter_bound = true;
        crate::ps2::initialize().map_err(|_| "ps2-init")?;
        supervisor.begin()?;
        // SAFETY: validated private ELF/payload pages and selected guarded
        // stack plus kernel syscall/IRQ/return paths are retained for boot.
        // The single CPU activates this root only once after all binding.
        unsafe {
            prepared.address_space.activate();
        }
        serial::write_line("PYTHOS:CORE:NORMAL_SESSION:ENTER");
        let outcome = user_mode::run_returnable_user_process(
            prepared.process,
            prepared.entry,
            prepared.stack.stack_start + prepared.stack.stack_len - 16,
            NORMAL_SESSION_BOOTSTRAP_ADDRESS,
            0,
        );
        if !boundary.restore_kernel() {
            fatal("kernel-root");
        }
        if !boundary.caller_cleared() {
            fatal("caller-transients");
        }
        let record = if outcome.is_ok() {
            Some(
                crate::memory::r#virtual::with_writable_physical_frame(
                    prepared.return_frame,
                    |page| {
                        // SAFETY: user execution ended and early deauthorization plus
                        // kernel CR3 were verified. The retained aligned page contains
                        // this 32-byte wire record; read by value, then validate it.
                        unsafe {
                            page.as_ptr()
                                .cast::<NormalSessionReturnV1>()
                                .read_volatile()
                        }
                    },
                )
                .map_err(|_| "return-page")?,
            )
        } else {
            None
        };
        recovery_decision(outcome, record)
    })();
    let invalid_return = if supervisor.state == State::Running {
        launch.err()
    } else {
        None
    };
    let decision = match launch {
        Ok(decision) => decision,
        Err(error) => {
            serial::write_str("PYTHOS:CORE:NORMAL_SESSION:LAUNCH_FAILED stage=");
            serial::write_line(error);
            RecoveryDecision {
                reason: RecoveryReason::Launch,
                fault: None,
            }
        }
    };
    if let Some(context) = decision.fault {
        serial::write_str("PYTHOS:CORE:NORMAL_SESSION:FAULT_CONTAINED principal:");
        serial::write_hex_u64_value(context.principal);
        serial::write_str(" vector:");
        serial::write_dec_u64_value(context.vector);
        serial::write_str(" rip:");
        serial::write_hex_u64_value(context.rip);
        serial::write_str(" rsp:");
        serial::write_hex_u64_value(context.rsp);
        serial::write_str(" cr2:");
        serial::write_hex_u64_value(context.cr2);
        serial::write_str("\r\n");
    }
    serial::write_str("PYTHOS:CORE:NORMAL_SESSION:RECOVERY reason=");
    serial::write_line(match decision.reason {
        RecoveryReason::Explicit => "explicit",
        RecoveryReason::NativeFault => "native-fault",
        RecoveryReason::Input => "input",
        RecoveryReason::Presentation => "presentation",
        RecoveryReason::Graph => "graph",
        RecoveryReason::Console => "console",
        RecoveryReason::Bootstrap => "bootstrap",
        RecoveryReason::Counter => "counter",
        RecoveryReason::Launch => "launch",
    });
    if let Err(error) = supervisor.recover(&mut boundary) {
        fatal(error);
    }
    if let Some(error) = invalid_return {
        fatal(error);
    }
    serial::write_line("PYTHOS:CORE:NORMAL_SESSION:CLEANUP_OK");
    crate::normal_boot::enter_recovery_shell(substrate)
}

#[cfg(not(test))]
pub(crate) fn fatal(stage: &'static str) -> ! {
    crate::serial::write_str("PYTHOS:CORE:NORMAL_SESSION:FATAL stage=");
    crate::serial::write_line(stage);
    loop {
        // SAFETY: unrecoverable gate failure has no active user continuation.
        // Single CPU disables interrupts and halts without touching any memory.
        unsafe {
            core::arch::asm!("cli", "hlt", options(nomem, nostack));
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecoveryReason {
    Explicit,
    NativeFault,
    Input,
    Presentation,
    Graph,
    Console,
    Bootstrap,
    Counter,
    Launch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RecoveryDecision {
    reason: RecoveryReason,
    fault: Option<UserFaultContext>,
}

fn recovery_decision(
    outcome: Result<(), UserModeError>,
    record: Option<NormalSessionReturnV1>,
) -> Result<RecoveryDecision, &'static str> {
    match outcome {
        Err(UserModeError::FaultContained(context))
            if context.principal
                == pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID
                && matches!(context.vector, 6 | 13 | 14)
                && context.rip != 0 =>
        {
            Ok(RecoveryDecision {
                reason: RecoveryReason::NativeFault,
                fault: Some(context),
            })
        }
        Err(_) => Err("return-outcome"),
        Ok(()) => {
            let record = record.ok_or("return-record")?;
            validate_normal_session_return(&record).map_err(|_| "return-record")?;
            let reason = match NormalSessionReturnReason::from_wire(record.reason) {
                Some(NormalSessionReturnReason::ExplicitRecovery) => RecoveryReason::Explicit,
                Some(NormalSessionReturnReason::Input) => RecoveryReason::Input,
                Some(NormalSessionReturnReason::Presentation) => RecoveryReason::Presentation,
                Some(NormalSessionReturnReason::Graph) => RecoveryReason::Graph,
                Some(NormalSessionReturnReason::Console) => RecoveryReason::Console,
                Some(NormalSessionReturnReason::Bootstrap) => RecoveryReason::Bootstrap,
                Some(NormalSessionReturnReason::CounterOverflow) => RecoveryReason::Counter,
                None => return Err("return-record"),
            };
            Ok(RecoveryDecision {
                reason,
                fault: None,
            })
        }
    }
}

trait RecoveryBoundary {
    fn restore_kernel(&mut self) -> bool;
    fn caller_cleared(&mut self) -> bool;
    fn revoke_grants(&mut self) -> bool;
    fn disable_presenter(&mut self) -> bool;
    fn queue_retained(&mut self) -> bool;
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Prepared,
    Running,
    Recovering,
    Recovered,
}
struct NormalSessionSupervisor {
    state: State,
}
impl NormalSessionSupervisor {
    fn new() -> Self {
        Self {
            state: State::Prepared,
        }
    }
    fn begin(&mut self) -> Result<(), &'static str> {
        if self.state != State::Prepared {
            return Err("already-launched");
        }
        self.state = State::Running;
        Ok(())
    }
    fn recover(&mut self, boundary: &mut impl RecoveryBoundary) -> Result<(), &'static str> {
        if !matches!(self.state, State::Prepared | State::Running) {
            return Err("already-recovered");
        }
        self.state = State::Recovering;
        if !boundary.restore_kernel() {
            return Err("kernel-root");
        }
        if !boundary.caller_cleared() {
            return Err("caller-transients");
        }
        let grants = boundary.revoke_grants();
        let presenter = boundary.disable_presenter();
        if !grants {
            return Err("grants");
        }
        if !presenter {
            return Err("presenter");
        }
        if !boundary.queue_retained() {
            return Err("input-owner");
        }
        self.state = State::Recovered;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_cleanup_composes_real_partial_grants_queue_ownership_and_presenter_tombstone() {
        use crate::{
            capabilities::{CapabilityTable, ResourceId, RightsMask},
            process_context::ActiveUserProcess,
            service_identity::ServiceId,
            session_input::SessionInputQueue,
            session_presentation::PresentationService,
            syscall::{NormalSessionGrants, normal_grant_test_support},
        };
        struct OwnedBoundary {
            table: CapabilityTable,
            queue: SessionInputQueue,
            grants: Option<NormalSessionGrants>,
            presenter: PresentationService,
            bound: bool,
        }
        impl RecoveryBoundary for OwnedBoundary {
            fn restore_kernel(&mut self) -> bool {
                true
            }
            fn caller_cleared(&mut self) -> bool {
                true
            }
            fn revoke_grants(&mut self) -> bool {
                self.grants.as_mut().is_none_or(|grants| {
                    normal_grant_test_support::revoke(&mut self.table, grants).is_ok()
                        && grants.is_revoked()
                })
            }
            fn disable_presenter(&mut self) -> bool {
                !self.bound || self.presenter.disable(ServiceId::from_raw(7)).is_ok()
            }
            fn queue_retained(&mut self) -> bool {
                if self.bound {
                    self.queue.session_ready(ServiceId::from_raw(7)).is_ok()
                } else {
                    self.queue.session_ready(ServiceId::from_raw(7)).is_err()
                }
            }
        }
        for available in 0..=4 {
            for stale in [false, true] {
                let process = ActiveUserProcess::new(
                    ServiceId::from_raw(7),
                    pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
                    1,
                );
                let mut boundary = OwnedBoundary {
                    table: CapabilityTable::new(),
                    queue: SessionInputQueue::new(),
                    grants: None,
                    presenter: PresentationService::new(),
                    bound: false,
                };
                let mut peer_handles = std::vec::Vec::new();
                for index in 0..32 - available {
                    peer_handles.push(
                        boundary
                            .table
                            .grant(
                                ServiceId::from_raw(9),
                                ResourceId::new(index),
                                RightsMask::new(RightsMask::READ),
                            )
                            .unwrap(),
                    );
                }
                boundary.grants =
                    normal_grant_test_support::grant(&mut boundary.table, process, &boundary.queue)
                        .ok();
                assert_eq!(boundary.grants.is_some(), available == 4);
                let mut pixels = std::vec![0u32; 640 * 480];
                let info = pythos_shared::boot_protocol::PythFramebufferInfo {
                    physical_base: 0x1000,
                    mapped_virtual_base: pixels.as_mut_ptr() as u64,
                    byte_length: (pixels.len() * 4) as u64,
                    width: 640,
                    height: 480,
                    pixels_per_scanline: 640,
                    pixel_format: pythos_shared::boot_protocol::PIXEL_FORMAT_RGB_RESERVED_8BIT,
                    red_mask: 0,
                    green_mask: 0,
                    blue_mask: 0,
                    reserved_mask: 0,
                };
                if let Some(grants) = &boundary.grants {
                    // SAFETY: this aligned Vec owns the complete mutable pixel
                    // surface until the boundary is retired; no alias writes it.
                    unsafe {
                        boundary.presenter.bind(
                            process.service_id(),
                            info,
                            crate::viewing::ViewingExtent::new(640, 480).unwrap(),
                        )
                    }
                    .unwrap();
                    boundary.bound = true;
                    if stale {
                        boundary
                            .table
                            .revoke(crate::capabilities::CapabilityHandle::from_parts(
                                grants.console().slot(),
                                grants.console().generation(),
                            ))
                            .unwrap();
                    }
                }
                let mut supervisor = NormalSessionSupervisor::new();
                assert_eq!(
                    supervisor.recover(&mut boundary).is_err(),
                    stale && available == 4
                );
                assert!(supervisor.begin().is_err());
                for (index, handle) in peer_handles.into_iter().enumerate() {
                    assert!(
                        boundary
                            .table
                            .validate(
                                ServiceId::from_raw(9),
                                handle,
                                ResourceId::new(index as u64),
                                RightsMask::new(RightsMask::READ)
                            )
                            .is_ok()
                    );
                }
                if boundary.bound {
                    assert_eq!(boundary.presenter.accepted_snapshot(), None);
                    assert!(
                        boundary
                            .presenter
                            .present(process.service_id(), 1, 0, 0, 0)
                            .is_err()
                    );
                    assert!(
                        boundary
                            .queue
                            .bind_session_consumer_quiescent(ServiceId::from_raw(8))
                            .is_err()
                    );
                }
            }
        }
    }

    #[test]
    fn normal_launch_record_is_validated_before_mapping_write() {
        use pythos_shared::{capability_abi::PackedCapability, pyth_runtime_abi::*};
        let mut graph = NormalSessionBootstrapV1::empty().graph;
        graph.magic = PYTH_GRAPH_BOOTSTRAP_MAGIC;
        graph.abi_major = 1;
        graph.import_count = 1;
        graph.package_ptr = NORMAL_SESSION_GRAPH_PACKAGE_ADDRESS;
        graph.package_len = 696;
        graph.instruction_budget = crate::pyth_runtime_launch::PYTH_GRAPH_DEFAULT_BUDGET;
        graph.result_ptr = NORMAL_SESSION_GRAPH_RESULT_ADDRESS;
        graph.imports[0] = PythGraphCapabilityBinding {
            import_slot: 0,
            resource_kind: 6,
            reserved0: 0,
            rights: 0x11,
            capability: PackedCapability::from_raw(4),
        };
        let caps = [1, 2, 3].map(PackedCapability::from_raw);
        let bootstrap = build_bootstrap(graph, 0x1234, caps).unwrap();
        assert_eq!(bootstrap.graph.instruction_budget, 128);
        assert_eq!(validate_normal_session_bootstrap(&bootstrap), Ok(()));
        assert!(build_bootstrap(graph, 0, caps).is_err());
        assert!(build_bootstrap(graph, 1, [caps[0], caps[0], caps[2]]).is_err());
        graph.result_ptr = NORMAL_SESSION_RETURN_ADDRESS;
        assert!(build_bootstrap(graph, 1, caps).is_err());
    }

    #[derive(Default)]
    struct Boundary {
        fail: Option<&'static str>,
        calls: std::vec::Vec<&'static str>,
    }
    impl Boundary {
        fn check(&mut self, name: &'static str) -> bool {
            self.calls.push(name);
            self.fail != Some(name)
        }
    }
    impl RecoveryBoundary for Boundary {
        fn restore_kernel(&mut self) -> bool {
            self.check("root")
        }
        fn caller_cleared(&mut self) -> bool {
            self.check("caller")
        }
        fn revoke_grants(&mut self) -> bool {
            self.check("grants")
        }
        fn disable_presenter(&mut self) -> bool {
            self.check("presenter")
        }
        fn queue_retained(&mut self) -> bool {
            self.check("queue")
        }
    }

    #[test]
    fn normal_cleanup_is_ordered_and_cannot_restart_or_run_twice() {
        let mut supervisor = NormalSessionSupervisor::new();
        assert!(supervisor.begin().is_ok());
        assert!(supervisor.begin().is_err());
        let mut boundary = Boundary::default();
        assert_eq!(supervisor.recover(&mut boundary), Ok(()));
        assert_eq!(
            boundary.calls,
            ["root", "caller", "grants", "presenter", "queue"]
        );
        assert!(supervisor.begin().is_err());
        assert!(supervisor.recover(&mut boundary).is_err());
        assert_eq!(boundary.calls.len(), 5);
    }

    #[test]
    fn normal_cleanup_fails_closed_at_each_boundary_and_still_retires_presenter_after_grant_error()
    {
        for failed in ["root", "caller", "grants", "presenter", "queue"] {
            let mut supervisor = NormalSessionSupervisor::new();
            let mut boundary = Boundary {
                fail: Some(failed),
                ..Boundary::default()
            };
            assert!(supervisor.recover(&mut boundary).is_err(), "{failed}");
            assert!(supervisor.begin().is_err());
            if failed == "grants" {
                assert!(boundary.calls.contains(&"presenter"));
            }
        }
    }

    #[test]
    fn normal_return_uses_typed_record_or_matching_native_fault() {
        use pythos_shared::normal_session_abi::*;
        let record = NormalSessionReturnV1 {
            magic: NORMAL_SESSION_RETURN_MAGIC,
            abi_major: 1,
            abi_minor: 0,
            reason: 1,
            reserved0: 0,
            service_id: NORMAL_SESSION_SERVICE_ID,
            reserved1: 0,
        };
        assert_eq!(
            recovery_decision(Ok(()), Some(record)),
            Ok(RecoveryDecision {
                reason: RecoveryReason::Explicit,
                fault: None,
            })
        );
        assert!(recovery_decision(Ok(()), None).is_err());
        for malformed in [
            NormalSessionReturnV1 {
                reserved1: 1,
                ..record
            },
            NormalSessionReturnV1 {
                reason: 0,
                ..record
            },
        ] {
            assert!(recovery_decision(Ok(()), Some(malformed)).is_err());
        }
        let fault = crate::user_mode::UserFaultContext {
            principal: pythos_shared::user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
            vector: 6,
            rip: 0x600010,
            rsp: 0x700010,
            cr2: 0,
        };
        assert_eq!(
            recovery_decision(
                Err(crate::user_mode::UserModeError::FaultContained(fault)),
                None
            ),
            Ok(RecoveryDecision {
                reason: RecoveryReason::NativeFault,
                fault: Some(fault),
            })
        );
        assert!(
            recovery_decision(
                Err(crate::user_mode::UserModeError::FaultContained(
                    crate::user_mode::UserFaultContext {
                        principal: 7,
                        ..fault
                    }
                )),
                None
            )
            .is_err()
        );
        assert!(
            recovery_decision(
                Err(crate::user_mode::UserModeError::DidNotReturn),
                Some(record)
            )
            .is_err()
        );
    }
}
