//! Checkpoint publication on the ADR 0110 proof's dedicated block image.

use crate::{
    block_device::{self, BlockDeviceInfo, SECTOR_SIZE},
    capabilities::{CapabilityHandle, CapabilityTable, RightsMask},
    serial,
    service_identity::ServiceId,
    storage_service::{STORAGE_RESOURCE_ID, StorageRequest, StorageService},
};
use pythos_shared::{
    sha256::sha256,
    vault_pulse::{EVENT_LEN, Event, Outcome, PROGRAM_LEN, Program, Status},
};

const IMAGE_SECTORS: u64 = 16 * 1024 * 1024 / SECTOR_SIZE as u64;
const PROGRAM_SECTOR: u64 = 2;
const SLOT_SECTORS: [u64; 2] = [3, 5];
const END_SECTOR: u64 = 7;
const RECEIPT_LEN: usize = 48;
const MAX_RECEIPTS: usize = 4;
type Sector = [u8; SECTOR_SIZE];

#[derive(Clone, Copy)]
pub(crate) struct Receipt {
    pub(crate) event: Event,
    pub(crate) generation: u64,
    pub(crate) x: u64,
    pub(crate) cursor: u8,
    pub(crate) status: Status,
}

#[derive(Clone, Copy)]
pub(crate) struct Checkpoint {
    pub(crate) generation: u64,
    pub(crate) task: u64,
    pub(crate) x: u64,
    pub(crate) cursor: u8,
    pub(crate) status: Status,
    pub(crate) active: Option<Event>,
    program_digest: [u8; 32],
    receipts: [Option<Receipt>; MAX_RECEIPTS],
}

impl Checkpoint {
    pub(crate) fn first(task: u64, program: Program, event: Event) -> Self {
        Self {
            generation: 1,
            task,
            x: program.initial_x(),
            cursor: program.entry(),
            status: Status::Active,
            active: Some(event),
            program_digest: sha256(program.bytes()),
            receipts: [None; MAX_RECEIPTS],
        }
    }

    pub(crate) fn program_matches(self, program: Program) -> bool {
        sha256(program.bytes()) == self.program_digest
            && program.has_address(self.cursor)
            && (program.initial_x()..=program.goal()).contains(&self.x)
            && match self.status {
                Status::Active => true,
                Status::Waiting => {
                    self.x < program.goal() && program.wait_kind(self.cursor).is_some()
                }
                Status::Finished => self.x == program.goal(),
            }
    }

    pub(crate) fn receipt_for(self, event: Event) -> Option<Receipt> {
        if let Some(active) = self.active.filter(|active| active.same_id(event)) {
            return Some(Receipt {
                event: active,
                generation: self.generation,
                x: self.x,
                cursor: self.cursor,
                status: Status::Active,
            });
        }
        self.receipts
            .into_iter()
            .flatten()
            .find(|receipt| receipt.event.same_id(event))
    }

    pub(crate) fn start_event(mut self, event: Event) -> Result<Self, &'static str> {
        if self.status != Status::Waiting
            || self.active.is_some()
            || self.receipt_for(event).is_some()
        {
            return Err("EVENT_STATE");
        }
        if self.receipts.iter().all(Option::is_some) {
            return Err("RECEIPT_CAPACITY");
        }
        self.active = Some(event);
        self.status = Status::Active;
        Ok(self)
    }

    pub(crate) fn advance(mut self, outcome: Outcome) -> Result<Self, &'static str> {
        if self.generation != outcome.generation {
            return Err("STALE_RESULT");
        }
        self.generation = self.generation.checked_add(1).ok_or("GENERATION")?;
        self.x = outcome.x;
        self.cursor = outcome.cursor;
        self.status = outcome.status;
        if outcome.status != Status::Active {
            let event = self.active.take().ok_or("MISSING_EVENT")?;
            let slot = self
                .receipts
                .iter_mut()
                .find(|slot| slot.is_none())
                .ok_or("RECEIPT_CAPACITY")?;
            *slot = Some(Receipt {
                event,
                generation: self.generation,
                x: self.x,
                cursor: self.cursor,
                status: self.status,
            });
        }
        Ok(self)
    }

    fn encode(self) -> Sector {
        let mut sector = [0; SECTOR_SIZE];
        sector[..8].copy_from_slice(b"PYVCKP01");
        sector[8..16].copy_from_slice(&self.generation.to_le_bytes());
        sector[16..24].copy_from_slice(&self.task.to_le_bytes());
        sector[24..32].copy_from_slice(&self.x.to_le_bytes());
        sector[32] = self.cursor;
        sector[33] = self.status as u8;
        sector[40..72].copy_from_slice(&self.program_digest);
        if let Some(event) = self.active {
            sector[72..96].copy_from_slice(&event.encode());
        }
        for (index, receipt) in self.receipts.into_iter().flatten().enumerate() {
            let start = 96 + index * RECEIPT_LEN;
            sector[start..start + EVENT_LEN].copy_from_slice(&receipt.event.encode());
            sector[start + 24..start + 32].copy_from_slice(&receipt.generation.to_le_bytes());
            sector[start + 32..start + 40].copy_from_slice(&receipt.x.to_le_bytes());
            sector[start + 40] = receipt.cursor;
            sector[start + 41] = receipt.status as u8;
            sector[34] += 1;
        }
        sector
    }

    fn decode(bytes: &Sector) -> Option<Self> {
        let count = usize::from(bytes[34]);
        if &bytes[..8] != b"PYVCKP01" || count > MAX_RECEIPTS {
            return None;
        }
        let mut checkpoint = Self {
            generation: number(bytes, 8),
            task: number(bytes, 16),
            x: number(bytes, 24),
            cursor: bytes[32],
            status: Status::decode(bytes[33]).ok()?,
            active: None,
            program_digest: bytes[40..72].try_into().unwrap(),
            receipts: [None; MAX_RECEIPTS],
        };
        if checkpoint.generation == 0 || checkpoint.task == 0 {
            return None;
        }
        if checkpoint.status == Status::Active {
            checkpoint.active = Some(Event::decode(&bytes[72..96]).ok()?);
        }
        let mut previous_generation = 0;
        for index in 0..count {
            let start = 96 + index * RECEIPT_LEN;
            let event = Event::decode(&bytes[start..start + EVENT_LEN]).ok()?;
            let generation = number(bytes, start + 24);
            let x = number(bytes, start + 32);
            let cursor = bytes[start + 40];
            let status = Status::decode(bytes[start + 41]).ok()?;
            if status == Status::Active
                || generation <= previous_generation
                || generation > checkpoint.generation
                || checkpoint.receipt_for(event).is_some()
                || x > checkpoint.x
            {
                return None;
            }
            checkpoint.receipts[index] = Some(Receipt {
                event,
                generation,
                x,
                cursor,
                status,
            });
            previous_generation = generation;
        }
        if checkpoint.status != Status::Active {
            let latest = checkpoint.receipts.into_iter().flatten().last()?;
            if (latest.generation, latest.x, latest.cursor, latest.status)
                != (
                    checkpoint.generation,
                    checkpoint.x,
                    checkpoint.cursor,
                    checkpoint.status,
                )
            {
                return None;
            }
        }
        (checkpoint.encode() == *bytes).then_some(checkpoint)
    }
}

pub(crate) struct Delivery {
    pub(crate) task: u64,
    pub(crate) event: Event,
    pub(crate) program: Option<Program>,
}

impl Delivery {
    fn decode(sector: &Sector) -> Option<Self> {
        let task = number(sector, 8);
        if &sector[..8] != b"PYVMSG01" || task == 0 || !zero(&sector[40 + PROGRAM_LEN..]) {
            return None;
        }
        let event = Event::decode(&sector[16..40]).ok()?;
        let bytes = &sector[40..40 + PROGRAM_LEN];
        let program = if zero(bytes) {
            None
        } else {
            Some(Program::decode(bytes.try_into().unwrap()).ok()?)
        };
        Some(Self {
            task,
            event,
            program,
        })
    }
}

pub(crate) struct Store {
    device: BlockDeviceInfo,
    service: StorageService,
    capabilities: CapabilityTable,
    holder: ServiceId,
    cap: CapabilityHandle,
    generation: u64,
    interruption: u8,
}

impl Store {
    pub(crate) fn open(device: BlockDeviceInfo, holder: ServiceId) -> Result<Self, &'static str> {
        if !matches!(device, BlockDeviceInfo::Virtio(_))
            || device.capacity_sectors() != IMAGE_SECTORS
        {
            return Err("PROOF_DEVICE");
        }
        let mut capabilities = CapabilityTable::new();
        let cap = capabilities
            .grant(
                holder,
                STORAGE_RESOURCE_ID,
                RightsMask::new(RightsMask::READ | RightsMask::WRITE),
            )
            .map_err(|_| "STORAGE_CAPABILITY")?;
        let mut store = Self {
            device,
            service: StorageService::new(device),
            capabilities,
            holder,
            cap,
            generation: 0,
            interruption: 0,
        };
        let header = store.read(0)?;
        if &header[..8] != b"PYVDSK01" || header[8] > 2 || !zero(&header[9..]) {
            return Err("PROOF_IMAGE");
        }
        store.interruption = header[8];
        Ok(store)
    }

    pub(crate) fn receive(&self) -> Result<Option<Delivery>, &'static str> {
        let sector = self.read(1)?;
        if zero(&sector) {
            return Ok(None);
        }
        let delivery = Delivery::decode(&sector);
        if delivery.is_none() {
            serial::write_line("PYTHOS:VAULT:REJECTED:MAILBOX");
        }
        Ok(delivery)
    }

    pub(crate) fn recover(&mut self) -> Result<Option<Checkpoint>, &'static str> {
        let mut newest = None;
        let mut committed = false;
        for (index, sector) in SLOT_SECTORS.into_iter().enumerate() {
            let marker = self.read(sector + 1)?;
            if zero(&marker) {
                continue;
            }
            committed = true;
            let body = self.read(sector)?;
            if let Some(checkpoint) = Checkpoint::decode(&body)
                && marker == commit_marker(checkpoint.generation, &body)
                && slot_index(checkpoint.generation) == index
                && newest.is_none_or(|old: Checkpoint| checkpoint.generation > old.generation)
            {
                newest = Some(checkpoint);
            }
        }
        if let Some(checkpoint) = newest {
            self.generation = checkpoint.generation;
        } else if committed {
            return Err("CORRUPT_CHECKPOINT");
        }
        Ok(newest)
    }

    pub(crate) fn saved_program(
        &self,
        checkpoint: Checkpoint,
    ) -> Result<Option<Program>, &'static str> {
        let sector = self.read(PROGRAM_SECTOR)?;
        if !zero(&sector[PROGRAM_LEN..]) {
            return Ok(None);
        }
        Ok(Program::decode(sector[..PROGRAM_LEN].try_into().unwrap())
            .ok()
            .filter(|program| checkpoint.program_matches(*program)))
    }

    pub(crate) fn admit_program(&self, program: Program) -> Result<(), &'static str> {
        if self.generation != 0 {
            return Err("PROGRAM_IMMUTABLE");
        }
        let mut body = [0; SECTOR_SIZE];
        body[..PROGRAM_LEN].copy_from_slice(program.bytes());
        self.write(PROGRAM_SECTOR, &body)?;
        if self.read(PROGRAM_SECTOR)? != body {
            return Err("PROGRAM_READBACK");
        }
        Ok(())
    }

    pub(crate) fn publish(
        &mut self,
        expected: u64,
        checkpoint: Checkpoint,
    ) -> Result<(), &'static str> {
        if self.generation != expected || expected.checked_add(1) != Some(checkpoint.generation) {
            return Err("STALE_GENERATION");
        }
        let sector = SLOT_SECTORS[slot_index(checkpoint.generation)];
        let body = checkpoint.encode();
        if Checkpoint::decode(&body).is_none() {
            return Err("CANDIDATE");
        }
        let marker = commit_marker(checkpoint.generation, &body);
        self.write(sector + 1, &[0; SECTOR_SIZE])?;
        self.write(sector, &body)?;
        if self.read(sector)? != body {
            return Err("CHECKPOINT_READBACK");
        }
        if self.interruption == 2 && checkpoint.generation == 4 {
            interruption_barrier("PYTHOS:VAULT:KILL_BEFORE_COMMIT");
        }
        self.write(sector + 1, &marker)?;
        if self.read(sector + 1)? != marker {
            return Err("COMMIT_READBACK");
        }
        self.generation = checkpoint.generation;
        Ok(())
    }

    pub(crate) fn after_pulse(&self) {
        if self.interruption == 1 && self.generation == 3 {
            interruption_barrier("PYTHOS:VAULT:KILL_AFTER_COMMIT");
        }
    }

    fn read(&self, sector: u64) -> Result<Sector, &'static str> {
        if sector >= END_SECTOR {
            return Err("STORAGE_RANGE");
        }
        self.service
            .authorize_request(
                &self.capabilities,
                self.holder,
                self.cap,
                StorageRequest::read(sector, 1),
            )
            .map_err(|_| "STORAGE_READ_AUTHORITY")?;
        block_device::read_sector(self.device, sector).map_err(|_| "STORAGE_READ")
    }

    fn write(&self, sector: u64, bytes: &Sector) -> Result<(), &'static str> {
        if !(PROGRAM_SECTOR..END_SECTOR).contains(&sector) {
            return Err("STORAGE_RANGE");
        }
        self.service
            .authorize_request(
                &self.capabilities,
                self.holder,
                self.cap,
                StorageRequest::write(sector, 1),
            )
            .map_err(|_| "STORAGE_WRITE_AUTHORITY")?;
        block_device::write_sector(self.device, sector, bytes).map_err(|_| "STORAGE_WRITE")
    }
}

fn number(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

fn slot_index(generation: u64) -> usize {
    (generation & 1 == 0) as usize
}

fn commit_marker(generation: u64, body: &Sector) -> Sector {
    let mut marker = [0; SECTOR_SIZE];
    marker[..8].copy_from_slice(b"PYVCMT01");
    marker[8..16].copy_from_slice(&generation.to_le_bytes());
    marker[16..48].copy_from_slice(&sha256(body));
    marker
}

fn interruption_barrier(marker: &str) -> ! {
    serial::write_line(marker);
    loop {
        core::hint::spin_loop();
    }
}
