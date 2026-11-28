//! Returnable ring-3 calls over disjoint, bounded payload pages.

use crate::{
    memory::{
        physical::PhysicalMemory,
        r#virtual::{
            KernelAddressSpace, RetainedUserAddressSpace, UserAddressSpace, UserPayloadMapping,
            with_writable_physical_frame,
        },
    },
    process_context::{self, ActiveUserProcess},
    runtime_loader,
    service_identity::ServiceId,
    user_elf, user_mode, user_stacks,
};
use pythos_shared::boot_protocol::PythBootInfo;

const INPUT_ADDRESS: u64 = 0x7300_0000;
const PAGE_SIZE: u64 = 4096;
const RESULT_ADDRESS: u64 = INPUT_ADDRESS + PAGE_SIZE;

pub(crate) struct ProofWorker<const INPUT: usize, const OUTPUT: usize> {
    process: ActiveUserProcess,
    root: RetainedUserAddressSpace,
    entry: u64,
    stack_top: u64,
    input_frame: u64,
    result_frame: u64,
}

impl<const INPUT: usize, const OUTPUT: usize> ProofWorker<INPUT, OUTPUT> {
    pub(crate) fn prepare(
        boot: &PythBootInfo,
        memory: &mut PhysicalMemory,
        holder: ServiceId,
        name: &[u8],
        principal: u64,
    ) -> Result<Self, &'static str> {
        if INPUT == 0 || OUTPUT == 0 || INPUT as u64 > PAGE_SIZE || OUTPUT as u64 > PAGE_SIZE {
            return Err("WORKER_PAYLOAD_SIZE");
        }
        let manifest =
            runtime_loader::load_named_user_program(boot, name).map_err(|_| "WORKER_MANIFEST")?;
        if manifest.principal_id() != principal {
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
            principal,
            manifest.elf_digest(),
            &image,
            stack,
        )
        .map_err(|_| "WORKER_CONTEXT")?;
        let mut copy_map = process.copy_map();
        copy_map
            .add_mapping(INPUT_ADDRESS, INPUT as u64, true, false)
            .map_err(|_| "INPUT_COPY_MAP")?;
        copy_map
            .add_mapping(RESULT_ADDRESS, OUTPUT as u64, true, true)
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

    pub(crate) fn service_id(&self) -> ServiceId {
        self.process.service_id()
    }

    pub(crate) fn exchange(
        &self,
        kernel: &KernelAddressSpace,
        input: &[u8; INPUT],
    ) -> Result<[u8; OUTPUT], &'static str> {
        with_writable_physical_frame(self.input_frame, |page| {
            page[..INPUT].copy_from_slice(input)
        })
        .map_err(|_| "INPUT_WRITE")?;
        with_writable_physical_frame(self.result_frame, |page| page[..OUTPUT].fill(0))
            .map_err(|_| "RESULT_CLEAR")?;
        // SAFETY: prepare validated the executable entry, guarded stack, and
        // separate payload mappings. The worker retains the mapped frames for
        // the single CPU's boot lifetime. Only this call activates its root.
        unsafe { self.root.activate() };
        let returned = user_mode::run_returnable_user_process(
            self.process,
            self.entry,
            self.stack_top,
            INPUT_ADDRESS,
            RESULT_ADDRESS,
        );
        // SAFETY: the caller holds the validated kernel root for the whole
        // call. The return trap is back on its retained supervisor stack and
        // no user instruction can access either payload during this copy.
        unsafe { kernel.activate() };
        returned.map_err(|_| "WORKER_RETURN")?;
        if process_context::current_caller().is_ok() {
            return Err("WORKER_CONTEXT_LEAK");
        }
        with_writable_physical_frame(self.result_frame, |page| page[..OUTPUT].try_into().unwrap())
            .map_err(|_| "RESULT_READ")
    }
}
