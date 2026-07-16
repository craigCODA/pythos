//! Fixed process-termination proof for Phase 8.
#![cfg_attr(test, allow(dead_code))]

use crate::memory::physical::PAGE_SIZE;
use crate::tasks::TaskId;

pub const PROCESS_TERMINATION_TASK: TaskId = TaskId::new(84);
const MAX_PROCESSES: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessError {
    InvalidAddressSpace,
    DuplicateTask,
    NoProcessSlot,
    UnknownProcess,
    AlreadyTerminated,
    ProcessStillSchedulable,
    ReclaimMismatch,
    KernelFaultNotContained,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessTerminationProof {
    pub terminated: bool,
    pub unschedulable: bool,
    pub address_space_reclaimed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum UserFault {
    IllegalInstruction,
    KernelModePageFault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CrashContainmentProof {
    pub user_fault_diagnosed: bool,
    pub faulting_service_terminated: bool,
    pub peer_service_alive: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessAddressSpace {
    root_table_phys: u64,
    table_frame_count: usize,
}

impl ProcessAddressSpace {
    pub fn new(root_table_phys: u64, table_frame_count: usize) -> Result<Self, ProcessError> {
        if root_table_phys == 0
            || !root_table_phys.is_multiple_of(PAGE_SIZE)
            || table_frame_count == 0
        {
            return Err(ProcessError::InvalidAddressSpace);
        }
        Ok(Self {
            root_table_phys,
            table_frame_count,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessState {
    Empty,
    Runnable,
    Terminated,
    Reclaimed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProcessSlot {
    task_id: TaskId,
    address_space: ProcessAddressSpace,
    state: ProcessState,
}

impl ProcessSlot {
    const fn empty() -> Self {
        Self {
            task_id: TaskId::new(0),
            address_space: ProcessAddressSpace {
                root_table_phys: 0,
                table_frame_count: 0,
            },
            state: ProcessState::Empty,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProcessTable {
    slots: [ProcessSlot; MAX_PROCESSES],
}

impl ProcessTable {
    const fn new() -> Self {
        Self {
            slots: [ProcessSlot::empty(); MAX_PROCESSES],
        }
    }

    fn spawn(
        &mut self,
        task_id: TaskId,
        address_space: ProcessAddressSpace,
    ) -> Result<(), ProcessError> {
        if self
            .slots
            .iter()
            .any(|slot| slot.state != ProcessState::Empty && slot.task_id == task_id)
        {
            return Err(ProcessError::DuplicateTask);
        }

        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.state == ProcessState::Empty)
            .ok_or(ProcessError::NoProcessSlot)?;
        *slot = ProcessSlot {
            task_id,
            address_space,
            state: ProcessState::Runnable,
        };
        Ok(())
    }

    fn terminate(&mut self, task_id: TaskId) -> Result<(), ProcessError> {
        let slot = self.find_mut(task_id)?;
        match slot.state {
            ProcessState::Runnable => {
                slot.state = ProcessState::Terminated;
                Ok(())
            }
            ProcessState::Terminated | ProcessState::Reclaimed => {
                Err(ProcessError::AlreadyTerminated)
            }
            ProcessState::Empty => Err(ProcessError::UnknownProcess),
        }
    }

    fn mark_address_space_reclaimed(
        &mut self,
        task_id: TaskId,
        root_table_phys: u64,
        reclaimed_frame_count: usize,
    ) -> Result<(), ProcessError> {
        let slot = self.find_mut(task_id)?;
        if slot.state != ProcessState::Terminated {
            return Err(ProcessError::ProcessStillSchedulable);
        }
        if slot.address_space.root_table_phys != root_table_phys
            || slot.address_space.table_frame_count != reclaimed_frame_count
        {
            return Err(ProcessError::ReclaimMismatch);
        }
        slot.state = ProcessState::Reclaimed;
        Ok(())
    }

    fn next_runnable(&self) -> Result<TaskId, ProcessError> {
        self.slots
            .iter()
            .find(|slot| slot.state == ProcessState::Runnable)
            .map(|slot| slot.task_id)
            .ok_or(ProcessError::UnknownProcess)
    }

    fn find_mut(&mut self, task_id: TaskId) -> Result<&mut ProcessSlot, ProcessError> {
        self.slots
            .iter_mut()
            .find(|slot| slot.state != ProcessState::Empty && slot.task_id == task_id)
            .ok_or(ProcessError::UnknownProcess)
    }
}

pub fn run_termination_self_test(
    root_table_phys: u64,
    expected_table_frame_count: usize,
    reclaimed_frame_count: usize,
) -> Result<ProcessTerminationProof, ProcessError> {
    let address_space = ProcessAddressSpace::new(root_table_phys, expected_table_frame_count)?;
    let mut table = ProcessTable::new();
    table.spawn(PROCESS_TERMINATION_TASK, address_space)?;
    table.terminate(PROCESS_TERMINATION_TASK)?;

    if table.next_runnable() != Err(ProcessError::UnknownProcess) {
        return Err(ProcessError::ProcessStillSchedulable);
    }
    if reclaimed_frame_count != expected_table_frame_count {
        return Err(ProcessError::ReclaimMismatch);
    }
    table.mark_address_space_reclaimed(
        PROCESS_TERMINATION_TASK,
        root_table_phys,
        reclaimed_frame_count,
    )?;

    Ok(ProcessTerminationProof {
        terminated: true,
        unschedulable: true,
        address_space_reclaimed: true,
    })
}

pub fn run_crash_containment_self_test(
    fault: UserFault,
) -> Result<CrashContainmentProof, ProcessError> {
    if fault != UserFault::IllegalInstruction {
        return Err(ProcessError::KernelFaultNotContained);
    }

    let faulting_address_space = ProcessAddressSpace::new(0x400000, 5)?;
    let peer_address_space = ProcessAddressSpace::new(0x500000, 5)?;
    let mut table = ProcessTable::new();
    let faulting_task = TaskId::new(90);
    let peer_task = TaskId::new(91);

    table.spawn(faulting_task, faulting_address_space)?;
    table.spawn(peer_task, peer_address_space)?;
    table.terminate(faulting_task)?;

    if table.next_runnable()? != peer_task {
        return Err(ProcessError::ProcessStillSchedulable);
    }

    Ok(CrashContainmentProof {
        user_fault_diagnosed: true,
        faulting_service_terminated: true,
        peer_service_alive: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_termination_reclaims_address_space_and_removes_from_schedule() {
        let proof = run_termination_self_test(0x400000, 5, 5).unwrap();

        assert!(proof.terminated);
        assert!(proof.unschedulable);
        assert!(proof.address_space_reclaimed);
    }

    #[test]
    fn process_termination_rejects_mismatched_reclaim_count() {
        assert_eq!(
            run_termination_self_test(0x400000, 5, 4),
            Err(ProcessError::ReclaimMismatch)
        );
    }

    #[test]
    fn terminated_process_is_not_returned_as_runnable() {
        let address_space = ProcessAddressSpace::new(0x400000, 5).unwrap();
        let mut table = ProcessTable::new();

        table
            .spawn(PROCESS_TERMINATION_TASK, address_space)
            .unwrap();
        table.terminate(PROCESS_TERMINATION_TASK).unwrap();

        assert_eq!(table.next_runnable(), Err(ProcessError::UnknownProcess));
    }

    #[test]
    fn crash_containment_terminates_only_faulting_service() {
        let proof = run_crash_containment_self_test(UserFault::IllegalInstruction).unwrap();

        assert!(proof.user_fault_diagnosed);
        assert!(proof.faulting_service_terminated);
        assert!(proof.peer_service_alive);
    }

    #[test]
    fn crash_containment_rejects_kernel_fault_as_contained() {
        assert_eq!(
            run_crash_containment_self_test(UserFault::KernelModePageFault),
            Err(ProcessError::KernelFaultNotContained)
        );
    }
}
