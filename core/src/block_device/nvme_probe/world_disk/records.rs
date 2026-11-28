//! One nonblocking record request on the graph profile's owned NVMe queue.

use crate::architecture::x86_64::{clock, timer};

use super::super::{Command, IO_TIMEOUT_MILLISECONDS, SECTOR_SIZE};
use super::WorldDisk;

const COMMAND_TIMEOUT_TICKS: u64 =
    (IO_TIMEOUT_MILLISECONDS * timer::TIMER_HZ as u64).div_ceil(1000) + 1;

pub(crate) enum RecordCompletion {
    /// Both the matching Write and the subsequent namespace Flush completed.
    Written,
    Read([u8; SECTOR_SIZE]),
}

#[derive(Clone, Copy)]
pub(super) enum RecordPhase {
    Read,
    Write,
    Flush,
}

#[derive(Clone, Copy)]
pub(super) enum RecordRequest {
    Idle,
    Pending { phase: RecordPhase, started: u64 },
    Stopped,
}

impl WorldDisk {
    pub(crate) fn begin_record_write(
        &mut self,
        record: u64,
        bytes: &[u8; SECTOR_SIZE],
    ) -> Result<(), &'static str> {
        self.require_record_idle()?;
        let lba = self.checkpoint_lba(record)?;
        let command = self.namespace.prepare_page_write(lba, bytes)?;
        self.start_record_command(RecordPhase::Write, command)
    }

    pub(crate) fn begin_record_read(&mut self, record: u64) -> Result<(), &'static str> {
        self.require_record_idle()?;
        let lba = self.record_lba(record)?;
        let command = self.namespace.read_command(lba)?;
        self.start_record_command(RecordPhase::Read, command)
    }

    /// Inspect the outstanding command once. Each call can consume one
    /// completion and submit its follow-up Flush. The boot clock and input
    /// IRQs can advance independently while the single caller owns this disk.
    pub(crate) fn poll_record(&mut self) -> Result<Option<RecordCompletion>, &'static str> {
        let (phase, started) = match self.request {
            RecordRequest::Idle => return Ok(None),
            RecordRequest::Pending { phase, started } => (phase, started),
            RecordRequest::Stopped => return Err("RECORD_STOPPED"),
        };
        let failure = match self.namespace.io.poll_completion(self.namespace.registers) {
            Ok(Some(completion)) if !completion.failed() => return self.finish_record(phase),
            Ok(Some(_)) => "COMMAND_STATUS",
            Ok(None)
                if clock::monotonic_ticks().raw().wrapping_sub(started) < COMMAND_TIMEOUT_TICKS =>
            {
                return Ok(None);
            }
            Ok(None) => "COMMAND_TIMEOUT",
            Err(reason) => reason,
        };
        self.cancel_record();
        Err(failure)
    }

    /// Permanently stop submissions on this handle. An outstanding command
    /// can still own its static DMA pages. They stay reserved until the driver
    /// disables the controller and confirms bus mastering off at callback exit.
    /// A write the controller already accepted can persist after cancellation.
    pub(crate) fn cancel_record(&mut self) {
        self.request = RecordRequest::Stopped;
    }

    pub(super) fn require_record_idle(&self) -> Result<(), &'static str> {
        match self.request {
            RecordRequest::Idle => Ok(()),
            RecordRequest::Pending { .. } => Err("RECORD_BUSY"),
            RecordRequest::Stopped => Err("RECORD_STOPPED"),
        }
    }

    fn start_record_command(
        &mut self,
        phase: RecordPhase,
        command: Command,
    ) -> Result<(), &'static str> {
        let started = clock::monotonic_ticks().raw();
        if started == 0 {
            self.cancel_record();
            return Err("NVME_TIMER_START");
        }
        if let Err(reason) = self.namespace.io.begin(self.namespace.registers, command) {
            self.cancel_record();
            return Err(reason);
        }
        self.request = RecordRequest::Pending { phase, started };
        Ok(())
    }

    fn finish_record(
        &mut self,
        phase: RecordPhase,
    ) -> Result<Option<RecordCompletion>, &'static str> {
        let completion = match phase {
            RecordPhase::Read => RecordCompletion::Read(self.namespace.data.copy_bytes(0)),
            RecordPhase::Write => {
                self.start_record_command(RecordPhase::Flush, Command::flush())?;
                return Ok(None);
            }
            RecordPhase::Flush => RecordCompletion::Written,
        };
        self.request = RecordRequest::Idle;
        Ok(Some(completion))
    }
}
