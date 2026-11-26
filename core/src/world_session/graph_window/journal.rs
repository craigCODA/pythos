//! One queued placement and two recoverable slots on the graph proof disk.

use crate::block_device::nvme_probe::{RecordCompletion, WorldDisk};
use pythos_shared::sha256::sha256;

use super::layout::{Drop, Pose};

type Record = [u8; 512];
const EMPTY: Record = [0; 512];
const SLOTS: [u64; 2] = [1, 3];

#[derive(Clone, Copy)]
pub(super) struct Placement {
    pub revision: u64,
    pub drop: Drop,
}

impl Placement {
    fn body(self, graph: &[u8; 32]) -> Record {
        let mut bytes = EMPTY;
        bytes[..8].copy_from_slice(b"PYGWPS01");
        bytes[8..16].copy_from_slice(&self.revision.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.drop.sequence.to_le_bytes());
        for (field, value) in bytes[24..36].chunks_exact_mut(4).zip([
            self.drop.pose.x,
            self.drop.pose.y,
            self.drop.pose.z,
        ]) {
            field.copy_from_slice(&value.to_le_bytes());
        }
        bytes[36..68].copy_from_slice(graph);
        bytes
    }

    fn decode(bytes: &Record, graph: &[u8; 32]) -> Option<Self> {
        if &bytes[..8] != b"PYGWPS01"
            || &bytes[36..68] != graph
            || bytes[68..].iter().any(|byte| *byte != 0)
        {
            return None;
        }
        let field = |offset| i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let pose = Pose {
            x: field(24),
            y: field(28),
            z: field(32),
        };
        let revision = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let sequence = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        (revision != 0 && pose.valid()).then_some(Self {
            revision,
            drop: Drop { sequence, pose },
        })
    }
}

fn marker(revision: u64, body: &Record) -> Record {
    let mut bytes = EMPTY;
    bytes[..8].copy_from_slice(b"PYGWCM01");
    bytes[8..16].copy_from_slice(&revision.to_le_bytes());
    bytes[16..48].copy_from_slice(&sha256(body));
    bytes
}

pub(super) struct Journal<'a> {
    disk: &'a mut WorldDisk,
    graph: [u8; 32],
    committed: Option<Placement>,
    pending: Option<Publication>,
    queued: Option<Drop>,
}

impl<'a> Journal<'a> {
    pub fn recover(disk: &'a mut WorldDisk, graph: [u8; 32]) -> Result<Self, &'static str> {
        let mut latest: Option<Placement> = None;
        let mut marked = false;
        for (index, slot) in SLOTS.into_iter().enumerate() {
            let commit = disk.read(slot + 1)?;
            if commit == EMPTY {
                continue;
            }
            marked = true;
            let body = disk.read(slot)?;
            if let Some(saved) = Placement::decode(&body, &graph)
                && saved.revision as usize & 1 == index
                && commit == marker(saved.revision, &body)
                && latest.is_none_or(|old| old.revision < saved.revision)
            {
                latest = Some(saved);
            }
        }
        if marked && latest.is_none() {
            return Err("GRAPH_WINDOW_CHECKPOINT");
        }
        Ok(Self {
            disk,
            graph,
            committed: latest,
            pending: None,
            queued: None,
        })
    }

    pub fn committed(&self) -> Option<Placement> {
        self.committed
    }

    pub fn saving(&self) -> bool {
        self.pending.is_some() || self.queued.is_some()
    }

    pub fn queue(&mut self, drop: Drop) {
        self.queued = Some(drop);
    }

    pub fn service(&mut self, now: u64) -> Result<Progress, &'static str> {
        if let Some(save) = &mut self.pending {
            let progress = save.poll(self.disk, now)?;
            if let Progress::Committed(placement) = progress {
                self.committed = Some(placement);
                self.pending = None;
            }
            return Ok(progress);
        }
        let Some(drop) = self.queued.take() else {
            return Ok(Progress::Idle);
        };
        if self.committed.is_some_and(|old| old.drop.pose == drop.pose) {
            return Ok(Progress::Idle);
        }
        let revision = self
            .committed
            .map_or(0, |old| old.revision)
            .checked_add(1)
            .ok_or("GRAPH_WINDOW_REVISION")?;
        let placement = Placement { revision, drop };
        let body = placement.body(&self.graph);
        self.pending = Some(Publication {
            phase: Phase::Starting,
            slot: SLOTS[revision as usize & 1],
            commit: marker(revision, &body),
            body,
            placement,
        });
        Ok(Progress::Pending(placement))
    }
}

pub(super) enum Progress {
    Idle,
    Pending(Placement),
    BodyReady(Placement),
    Committed(Placement),
}

#[derive(Clone, Copy)]
enum Phase {
    Starting,
    Clearing,
    BodyWrite,
    BodyCheck,
    BeforeMarker(u64),
    MarkerWrite,
    MarkerCheck,
}

struct Publication {
    placement: Placement,
    slot: u64,
    body: Record,
    commit: Record,
    phase: Phase,
}

impl Publication {
    fn poll(&mut self, disk: &mut WorldDisk, now: u64) -> Result<Progress, &'static str> {
        match self.phase {
            Phase::Starting => {
                disk.begin_record_write(self.slot + 1, &EMPTY)?;
                self.phase = Phase::Clearing;
                return Ok(Progress::Idle);
            }
            Phase::BeforeMarker(due) => {
                if now >= due {
                    disk.begin_record_write(self.slot + 1, &self.commit)?;
                    self.phase = Phase::MarkerWrite;
                }
                return Ok(Progress::Idle);
            }
            _ => {}
        }
        let Some(response) = disk.poll_record()? else {
            return Ok(Progress::Idle);
        };
        self.phase = match (self.phase, response) {
            (Phase::Clearing, RecordCompletion::Written) => {
                disk.begin_record_write(self.slot, &self.body)?;
                Phase::BodyWrite
            }
            (Phase::BodyWrite, RecordCompletion::Written) => {
                disk.begin_record_read(self.slot)?;
                Phase::BodyCheck
            }
            (Phase::BodyCheck, RecordCompletion::Read(body)) if body == self.body => {
                self.phase = Phase::BeforeMarker(now.saturating_add(disk.interrupt_before()));
                return Ok(Progress::BodyReady(self.placement));
            }
            (Phase::MarkerWrite, RecordCompletion::Written) => {
                disk.begin_record_read(self.slot + 1)?;
                Phase::MarkerCheck
            }
            (Phase::MarkerCheck, RecordCompletion::Read(commit)) if commit == self.commit => {
                return Ok(Progress::Committed(self.placement));
            }
            _ => return Err("GRAPH_WINDOW_SAVE_READBACK"),
        };
        Ok(Progress::Idle)
    }
}
