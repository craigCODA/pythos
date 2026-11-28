//! Durable world snapshots on marked Virtio or NVMe proof storage.

#[cfg(feature = "world-session-nvme")]
use crate::block_device::nvme_probe::WorldDisk;
#[cfg(not(feature = "world-session-nvme"))]
use crate::block_device::{self, BlockDeviceInfo as WorldDisk};

use crate::{
    block_device::SECTOR_SIZE,
    capabilities::{CapabilityHandle, CapabilityTable, RightsMask},
    serial,
    service_identity::ServiceId,
    storage_service::{STORAGE_RESOURCE_ID, StorageRequest, StorageService},
};
use pythos_shared::{
    sha256::sha256,
    world_session::{WORLD_LEN, World},
};

type Sector = [u8; SECTOR_SIZE];
const SLOTS: [u64; 2] = [1, 3];
const END_SECTOR: u64 = 5;
#[cfg(not(feature = "world-session-nvme"))]
const IMAGE_SECTORS: u64 = (16 * 1024 * 1024 / SECTOR_SIZE) as u64;
const EMPTY: Sector = [0; SECTOR_SIZE];

pub(super) struct Store {
    device: WorldDisk,
    service: StorageService,
    capabilities: CapabilityTable,
    holder: ServiceId,
    handle: CapabilityHandle,
    revision: u64,
    interrupt_before: u64,
}

impl Store {
    pub(super) fn open(device: WorldDisk, holder: ServiceId) -> Result<Self, &'static str> {
        #[cfg(not(feature = "world-session-nvme"))]
        let interrupt_before = {
            if !matches!(device, WorldDisk::Virtio(_)) || device.capacity_sectors() != IMAGE_SECTORS
            {
                return Err("WORLD_DEVICE");
            }
            let header = block_device::read_sector(device, 0).map_err(|_| "WORLD_READ")?;
            if &header[..8] != b"PYWDSK01" || header[16..].iter().any(|byte| *byte != 0) {
                return Err("WORLD_IMAGE");
            }
            u64::from_le_bytes(header[8..16].try_into().unwrap())
        };
        #[cfg(feature = "world-session-nvme")]
        let interrupt_before = {
            if device.capacity_sectors() < END_SECTOR {
                return Err("WORLD_DEVICE");
            }
            device.interrupt_before()
        };
        let mut capabilities = CapabilityTable::new();
        let handle = capabilities
            .grant(
                holder,
                STORAGE_RESOURCE_ID,
                RightsMask::new(RightsMask::READ | RightsMask::WRITE),
            )
            .map_err(|_| "WORLD_CAPABILITY")?;
        Ok(Self {
            device,
            service: StorageService::with_capacity(END_SECTOR),
            capabilities,
            holder,
            handle,
            revision: 0,
            interrupt_before,
        })
    }

    pub(super) fn recover(&mut self) -> Result<Option<World>, &'static str> {
        let mut newest = None;
        let mut committed = false;
        for (index, address) in SLOTS.into_iter().enumerate() {
            let marker = self.read(address + 1)?;
            if marker == EMPTY {
                continue;
            }
            committed = true;
            let body = self.read(address)?;
            if let Ok(world) = World::decode(body[..WORLD_LEN].try_into().unwrap())
                && world.revision != 0
                && slot(world.revision) == index
                && marker == commit_marker(world.revision, &body)
                && body[WORLD_LEN..].iter().all(|byte| *byte == 0)
                && newest.is_none_or(|old: World| old.revision < world.revision)
            {
                newest = Some(world);
            }
        }
        if let Some(world) = newest {
            self.revision = world.revision;
        } else if committed {
            return Err("WORLD_CHECKPOINT");
        }
        Ok(newest)
    }

    pub(super) fn publish(&mut self, old: u64, world: World) -> Result<(), &'static str> {
        if old != self.revision || old.checked_add(1) != Some(world.revision) {
            return Err("WORLD_STALE");
        }
        let address = SLOTS[slot(world.revision)];
        let mut body = EMPTY;
        body[..WORLD_LEN].copy_from_slice(&world.encode());
        let marker = commit_marker(world.revision, &body);
        self.write(address + 1, &EMPTY)?;
        self.write(address, &body)?;
        if self.read(address)? != body {
            return Err("WORLD_BODY_READBACK");
        }
        if world.revision == self.interrupt_before {
            serial::write_str("PYTHOS:WORLD:BEFORE_COMMIT revision:");
            serial::write_dec_u64_value(world.revision);
            serial::write_line("");
            loop {
                core::hint::spin_loop();
            }
        }
        self.write(address + 1, &marker)?;
        if self.read(address + 1)? != marker {
            return Err("WORLD_MARKER_READBACK");
        }
        self.revision = world.revision;
        Ok(())
    }

    fn read(&mut self, sector: u64) -> Result<Sector, &'static str> {
        if sector >= END_SECTOR {
            return Err("WORLD_RANGE");
        }
        self.service
            .authorize_request(
                &self.capabilities,
                self.holder,
                self.handle,
                StorageRequest::read(sector, 1),
            )
            .map_err(|_| "WORLD_READ_AUTHORITY")?;
        #[cfg(not(feature = "world-session-nvme"))]
        {
            block_device::read_sector(self.device, sector).map_err(|_| "WORLD_READ")
        }
        #[cfg(feature = "world-session-nvme")]
        {
            self.device.read(sector)
        }
    }

    fn write(&mut self, sector: u64, bytes: &Sector) -> Result<(), &'static str> {
        if !(1..END_SECTOR).contains(&sector) {
            return Err("WORLD_RANGE");
        }
        self.service
            .authorize_request(
                &self.capabilities,
                self.holder,
                self.handle,
                StorageRequest::write(sector, 1),
            )
            .map_err(|_| "WORLD_WRITE_AUTHORITY")?;
        #[cfg(not(feature = "world-session-nvme"))]
        {
            block_device::write_sector(self.device, sector, bytes).map_err(|_| "WORLD_WRITE")
        }
        #[cfg(feature = "world-session-nvme")]
        {
            self.device.write(sector, bytes)
        }
    }
}

fn slot(revision: u64) -> usize {
    (revision & 1) as usize
}

fn commit_marker(revision: u64, body: &Sector) -> Sector {
    let mut marker = EMPTY;
    marker[..8].copy_from_slice(b"PYWCMT01");
    marker[8..16].copy_from_slice(&revision.to_le_bytes());
    marker[16..48].copy_from_slice(&sha256(body));
    marker
}
