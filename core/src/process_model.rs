//! Phase 9 general process-model proofs.
//!
//! This module ties the dynamic ELF loader to the existing fault, capability,
//! and copy-boundary mechanisms. It does not define package loading, storage
//! backed program loading, or a scheduler change.
#![cfg_attr(test, allow(dead_code))]

use crate::{process, syscall, user_copy, user_elf, user_mode};

pub const REQUIRED_ADVERSARIAL_PROGRAMS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessModelError {
    Process(process::ProcessError),
    Syscall(syscall::SyscallError),
    UserCopy(user_copy::UserCopyError),
    UserMode(user_mode::UserModeError),
    NotEnoughPrograms,
    LoadedProgramMismatch,
    ProofFailed,
}

impl From<process::ProcessError> for ProcessModelError {
    fn from(error: process::ProcessError) -> Self {
        Self::Process(error)
    }
}

impl From<syscall::SyscallError> for ProcessModelError {
    fn from(error: syscall::SyscallError) -> Self {
        Self::Syscall(error)
    }
}

impl From<user_copy::UserCopyError> for ProcessModelError {
    fn from(error: user_copy::UserCopyError) -> Self {
        Self::UserCopy(error)
    }
}

impl From<user_mode::UserModeError> for ProcessModelError {
    fn from(error: user_mode::UserModeError) -> Self {
        Self::UserMode(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneralFaultIsolationProof {
    pub dynamic_elf_loaded: bool,
    pub dynamic_user_fault: bool,
    pub faulting_service_terminated: bool,
    pub peer_service_alive: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessModelAdversarialProof {
    pub variants_loaded: bool,
    pub program_ran: bool,
    pub forged_capability_denied: bool,
    pub bad_syscall_pointer_denied: bool,
    pub hardware_access_denied: bool,
}

pub fn prove_general_fault_isolation(
    expected_entry: u64,
    expected_segment_count: usize,
    loaded: user_elf::LoadedUserElf,
    fault_result: Result<(), user_mode::UserModeError>,
) -> Result<GeneralFaultIsolationProof, ProcessModelError> {
    if loaded.entry() != expected_entry
        || loaded.segment_count() != expected_segment_count
        || !loaded.bss_zeroed()
    {
        return Err(ProcessModelError::LoadedProgramMismatch);
    }
    fault_result?;

    let containment =
        process::run_crash_containment_self_test(process::UserFault::DynamicIllegalInstruction)?;
    let proof = GeneralFaultIsolationProof {
        dynamic_elf_loaded: true,
        dynamic_user_fault: containment.user_fault_diagnosed,
        faulting_service_terminated: containment.faulting_service_terminated,
        peer_service_alive: containment.peer_service_alive,
    };
    if !proof.dynamic_user_fault || !proof.faulting_service_terminated || !proof.peer_service_alive
    {
        return Err(ProcessModelError::ProofFailed);
    }
    Ok(proof)
}

pub fn prove_adversarial_suite(
    loaded_programs: usize,
    run_result: Result<(), user_mode::UserModeError>,
    bad_pointer_result: Result<(), user_mode::UserModeError>,
    hardware_result: Result<(), user_mode::UserModeError>,
) -> Result<ProcessModelAdversarialProof, ProcessModelError> {
    if loaded_programs < REQUIRED_ADVERSARIAL_PROGRAMS {
        return Err(ProcessModelError::NotEnoughPrograms);
    }
    run_result?;
    bad_pointer_result?;
    hardware_result?;

    let boundary = syscall::run_boundary_capability_self_test()?;
    let copy = user_copy::run_self_test()?;
    let bad_pointer_containment =
        process::run_crash_containment_self_test(process::UserFault::DynamicBadPointer)?;
    let hardware_containment =
        process::run_crash_containment_self_test(process::UserFault::DynamicPrivilegedInstruction)?;

    let proof = ProcessModelAdversarialProof {
        variants_loaded: true,
        program_ran: true,
        forged_capability_denied: boundary.forged_handle_denied,
        bad_syscall_pointer_denied: copy.out_of_range_denied
            && bad_pointer_containment.user_fault_diagnosed,
        hardware_access_denied: boundary.direct_hardware_denied
            && hardware_containment.user_fault_diagnosed,
    };
    if !proof.forged_capability_denied
        || !proof.bad_syscall_pointer_denied
        || !proof.hardware_access_denied
    {
        return Err(ProcessModelError::ProofFailed);
    }
    Ok(proof)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded() -> user_elf::LoadedUserElf {
        user_elf::LoadedUserElf::new(0x0040_0000, 2, true)
    }

    #[test]
    fn general_fault_isolation_requires_loaded_dynamic_elf_and_contained_fault() {
        let proof = prove_general_fault_isolation(0x0040_0000, 2, loaded(), Ok(())).unwrap();

        assert!(proof.dynamic_elf_loaded);
        assert!(proof.dynamic_user_fault);
        assert!(proof.faulting_service_terminated);
        assert!(proof.peer_service_alive);
    }

    #[test]
    fn general_fault_isolation_rejects_unexecuted_fault_probe() {
        assert_eq!(
            prove_general_fault_isolation(
                0x0040_0000,
                2,
                loaded(),
                Err(user_mode::UserModeError::DidNotReturn)
            ),
            Err(ProcessModelError::UserMode(
                user_mode::UserModeError::DidNotReturn
            ))
        );
    }

    #[test]
    fn general_fault_isolation_rejects_loaded_program_mismatch() {
        assert_eq!(
            prove_general_fault_isolation(0x0040_1000, 2, loaded(), Ok(())),
            Err(ProcessModelError::LoadedProgramMismatch)
        );
    }

    #[test]
    fn adversarial_suite_requires_program_variants_and_denials() {
        let proof =
            prove_adversarial_suite(REQUIRED_ADVERSARIAL_PROGRAMS, Ok(()), Ok(()), Ok(())).unwrap();

        assert!(proof.variants_loaded);
        assert!(proof.program_ran);
        assert!(proof.forged_capability_denied);
        assert!(proof.bad_syscall_pointer_denied);
        assert!(proof.hardware_access_denied);
    }

    #[test]
    fn adversarial_suite_rejects_missing_program_variant() {
        assert_eq!(
            prove_adversarial_suite(REQUIRED_ADVERSARIAL_PROGRAMS - 1, Ok(()), Ok(()), Ok(())),
            Err(ProcessModelError::NotEnoughPrograms)
        );
    }

    #[test]
    fn adversarial_suite_rejects_unexecuted_program_variant() {
        assert_eq!(
            prove_adversarial_suite(
                REQUIRED_ADVERSARIAL_PROGRAMS,
                Err(user_mode::UserModeError::DidNotReturn),
                Ok(()),
                Ok(())
            ),
            Err(ProcessModelError::UserMode(
                user_mode::UserModeError::DidNotReturn
            ))
        );
    }
}
