//! Phase 10 dynamic typed-object count proof.
#![cfg_attr(test, allow(dead_code))]
// Some items are only used by the normal-boot object-service path (a
// `verify`-excluded set of modules), so they are legitimately unused under
// `--features verify`.
#![cfg_attr(feature = "verify", allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::{
    shell_objects::ObjectId,
    storage_allocator::{AllocatorError, BlockAllocator, BlockExtent},
    typed_object_format::TypedObjectRecord,
};
use pythos_shared::object_shell_abi::MAX_QUERY_RESULTS;

pub const MAX_DYNAMIC_OBJECTS: usize = MAX_QUERY_RESULTS + 1;
const DYNAMIC_OBJECT_BASE_SECTOR: u64 = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicObjectError {
    Allocator(AllocatorError),
    ObjectTableFull,
    DuplicateObject,
    UnknownObject,
}

impl From<AllocatorError> for DynamicObjectError {
    fn from(error: AllocatorError) -> Self {
        Self::Allocator(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DynamicObjectSlot {
    object: TypedObjectRecord,
    extent: BlockExtent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicObjectRecord {
    pub object: TypedObjectRecord,
    pub extent: BlockExtent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicObjectStore {
    allocator: BlockAllocator,
    objects: [Option<DynamicObjectSlot>; MAX_DYNAMIC_OBJECTS],
}

impl DynamicObjectStore {
    pub fn new(base_sector: u64, block_count: u16) -> Result<Self, DynamicObjectError> {
        Ok(Self {
            allocator: BlockAllocator::new(base_sector, block_count)?,
            objects: [None; MAX_DYNAMIC_OBJECTS],
        })
    }

    pub fn create_object(
        &mut self,
        object: TypedObjectRecord,
    ) -> Result<BlockExtent, DynamicObjectError> {
        if self.find_index(object.object_id()).is_some() {
            return Err(DynamicObjectError::DuplicateObject);
        }
        let slot_index = self
            .free_slot()
            .ok_or(DynamicObjectError::ObjectTableFull)?;
        let extent = self.allocator.allocate(1)?;
        self.objects[slot_index] = Some(DynamicObjectSlot { object, extent });
        Ok(extent)
    }

    pub fn object(self, object_id: ObjectId) -> Option<TypedObjectRecord> {
        self.find_index(object_id)
            .and_then(|index| self.objects[index].map(|slot| slot.object))
    }

    pub fn replace_object(&mut self, object: TypedObjectRecord) -> Result<(), DynamicObjectError> {
        let index = self
            .find_index(object.object_id())
            .ok_or(DynamicObjectError::UnknownObject)?;
        let slot = self.objects[index].ok_or(DynamicObjectError::UnknownObject)?;
        self.objects[index] = Some(DynamicObjectSlot {
            object,
            extent: slot.extent,
        });
        Ok(())
    }

    pub fn delete_object(&mut self, object_id: ObjectId) -> Result<(), DynamicObjectError> {
        let index = self
            .find_index(object_id)
            .ok_or(DynamicObjectError::UnknownObject)?;
        let slot = self.objects[index].ok_or(DynamicObjectError::UnknownObject)?;
        self.allocator.free(slot.extent)?;
        self.objects[index] = None;
        Ok(())
    }

    pub fn object_count(self) -> usize {
        let mut count = 0;
        let mut index = 0;
        while index < MAX_DYNAMIC_OBJECTS {
            if self.objects[index].is_some() {
                count += 1;
            }
            index += 1;
        }
        count
    }

    pub fn allocated_block_count(self) -> u16 {
        self.allocator.used_blocks()
    }

    pub fn extent_for(self, object_id: ObjectId) -> Option<BlockExtent> {
        self.find_index(object_id)
            .and_then(|index| self.objects[index].map(|slot| slot.extent))
    }

    pub fn object_records(self) -> [Option<DynamicObjectRecord>; MAX_DYNAMIC_OBJECTS] {
        let mut records = [None; MAX_DYNAMIC_OBJECTS];
        let mut index = 0;
        while index < MAX_DYNAMIC_OBJECTS {
            records[index] = self.objects[index].map(|slot| DynamicObjectRecord {
                object: slot.object,
                extent: slot.extent,
            });
            index += 1;
        }
        records
    }

    pub const fn allocator_bitmap(self) -> u64 {
        self.allocator.committed_bitmap()
    }

    pub fn restore_from_records(
        base_sector: u64,
        block_count: u16,
        bitmap: u64,
        records: [Option<DynamicObjectRecord>; MAX_DYNAMIC_OBJECTS],
    ) -> Result<Self, DynamicObjectError> {
        let allocator = BlockAllocator::restore_from_bitmap(base_sector, block_count, bitmap)?;
        let mut objects = [None; MAX_DYNAMIC_OBJECTS];
        let mut seen_bitmap = 0u64;
        let mut index = 0;
        while index < MAX_DYNAMIC_OBJECTS {
            if let Some(record) = records[index] {
                let extent_bitmap = extent_bitmap(record.extent, block_count)?;
                if seen_bitmap & extent_bitmap != 0 || bitmap & extent_bitmap != extent_bitmap {
                    return Err(DynamicObjectError::Allocator(AllocatorError::InvalidExtent));
                }
                if object_id_seen(records, index, record.object.object_id()) {
                    return Err(DynamicObjectError::DuplicateObject);
                }
                seen_bitmap |= extent_bitmap;
                objects[index] = Some(DynamicObjectSlot {
                    object: record.object,
                    extent: record.extent,
                });
            }
            index += 1;
        }
        if seen_bitmap != bitmap {
            return Err(DynamicObjectError::Allocator(AllocatorError::InvalidExtent));
        }
        Ok(Self { allocator, objects })
    }

    fn find_index(self, object_id: ObjectId) -> Option<usize> {
        let mut index = 0;
        while index < MAX_DYNAMIC_OBJECTS {
            if let Some(slot) = self.objects[index]
                && slot.object.object_id() == object_id
            {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    fn free_slot(self) -> Option<usize> {
        let mut index = 0;
        while index < MAX_DYNAMIC_OBJECTS {
            if self.objects[index].is_none() {
                return Some(index);
            }
            index += 1;
        }
        None
    }
}

fn extent_bitmap(extent: BlockExtent, block_count: u16) -> Result<u64, DynamicObjectError> {
    if extent.block_count() == 0 || block_count > 64 {
        return Err(DynamicObjectError::Allocator(AllocatorError::InvalidExtent));
    }
    let end = extent
        .start_block()
        .checked_add(extent.block_count())
        .ok_or(DynamicObjectError::Allocator(AllocatorError::InvalidExtent))?;
    if end > block_count {
        return Err(DynamicObjectError::Allocator(AllocatorError::InvalidExtent));
    }
    let mut bitmap = 0u64;
    let mut offset = 0;
    while offset < extent.block_count() {
        bitmap |= 1u64 << (extent.start_block() + offset);
        offset += 1;
    }
    Ok(bitmap)
}

fn object_id_seen(
    records: [Option<DynamicObjectRecord>; MAX_DYNAMIC_OBJECTS],
    before_index: usize,
    object_id: ObjectId,
) -> bool {
    let mut index = 0;
    while index < before_index {
        if let Some(record) = records[index]
            && record.object.object_id() == object_id
        {
            return true;
        }
        index += 1;
    }
    false
}

pub fn run_self_test() -> Result<(), DynamicObjectError> {
    use crate::shell_objects::ObjectKind;
    use crate::typed_object_format::TypedObjectField;

    let mut store = DynamicObjectStore::new(DYNAMIC_OBJECT_BASE_SECTOR, 8)?;
    let mut first =
        TypedObjectRecord::new(ObjectId::new(0x8001), ObjectKind::ServiceMonitorWindow, 1);
    first
        .push_field(
            TypedObjectField::new(1, 1, b"svc").map_err(|_| DynamicObjectError::UnknownObject)?,
        )
        .map_err(|_| DynamicObjectError::UnknownObject)?;
    let second = TypedObjectRecord::new(ObjectId::new(0x8002), ObjectKind::PythonConsoleWindow, 1);

    let first_extent = store.create_object(first)?;
    let second_extent = store.create_object(second)?;
    if first_extent.start_block() != 0
        || second_extent.start_block() != 1
        || store.object_count() != 2
        || store.allocated_block_count() != 2
    {
        return Err(DynamicObjectError::UnknownObject);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:DYNAMIC_OBJECT:CREATED");

    store.delete_object(ObjectId::new(0x8001))?;
    if store.object_count() != 1
        || store.allocated_block_count() != 1
        || store.extent_for(ObjectId::new(0x8001)).is_some()
        || store.extent_for(ObjectId::new(0x8002)) != Some(second_extent)
    {
        return Err(DynamicObjectError::UnknownObject);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:DYNAMIC_OBJECT:DELETED");

    Ok(())
}

pub fn run_fragmentation_self_test() -> Result<(), DynamicObjectError> {
    use crate::shell_objects::ObjectKind;

    let mut store = DynamicObjectStore::new(DYNAMIC_OBJECT_BASE_SECTOR + 16, 8)?;
    let first = TypedObjectRecord::new(ObjectId::new(0x8101), ObjectKind::ServiceMonitorWindow, 1);
    let middle = TypedObjectRecord::new(ObjectId::new(0x8102), ObjectKind::PythonConsoleWindow, 1);
    let tail = TypedObjectRecord::new(ObjectId::new(0x8103), ObjectKind::SettingsPanelWindow, 1);
    let replacement = TypedObjectRecord::new(
        ObjectId::new(0x8104),
        ObjectKind::ApplicationLauncherWindow,
        1,
    );

    let first_extent = store.create_object(first)?;
    let middle_extent = store.create_object(middle)?;
    let tail_extent = store.create_object(tail)?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:FRAGMENTATION:POLICY_RECORDED");

    store.delete_object(ObjectId::new(0x8102))?;
    let replacement_extent = store.create_object(replacement)?;
    if first_extent.start_block() != 0
        || middle_extent.start_block() != 1
        || tail_extent.start_block() != 2
        || replacement_extent.start_block() != middle_extent.start_block()
        || store.object_count() != 3
        || store.allocated_block_count() != 3
    {
        return Err(DynamicObjectError::UnknownObject);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:FRAGMENTATION:FREED_BLOCK_REUSED");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_objects::{ObjectId, ObjectKind};
    use crate::typed_object_format::TypedObjectRecord;

    #[test]
    fn store_creates_multiple_typed_objects() {
        let mut store = DynamicObjectStore::new(64, 8).unwrap();

        store
            .create_object(TypedObjectRecord::new(
                ObjectId::new(0x8001),
                ObjectKind::ServiceMonitorWindow,
                1,
            ))
            .unwrap();
        store
            .create_object(TypedObjectRecord::new(
                ObjectId::new(0x8002),
                ObjectKind::PythonConsoleWindow,
                1,
            ))
            .unwrap();

        assert_eq!(store.object_count(), 2);
        assert_eq!(store.allocated_block_count(), 2);
    }

    #[test]
    fn deleted_object_releases_its_extent() {
        let mut store = DynamicObjectStore::new(64, 8).unwrap();

        let first = store
            .create_object(TypedObjectRecord::new(
                ObjectId::new(0x8001),
                ObjectKind::ServiceMonitorWindow,
                1,
            ))
            .unwrap();
        let second = store
            .create_object(TypedObjectRecord::new(
                ObjectId::new(0x8002),
                ObjectKind::PythonConsoleWindow,
                1,
            ))
            .unwrap();

        store.delete_object(ObjectId::new(0x8001)).unwrap();

        assert_eq!(first.start_block(), 0);
        assert_eq!(second.start_block(), 1);
        assert_eq!(store.object_count(), 1);
        assert_eq!(store.allocated_block_count(), 1);
    }

    #[test]
    fn duplicate_object_id_is_denied() {
        let mut store = DynamicObjectStore::new(64, 8).unwrap();
        let record =
            TypedObjectRecord::new(ObjectId::new(0x8001), ObjectKind::ServiceMonitorWindow, 1);

        store.create_object(record).unwrap();

        assert_eq!(
            store.create_object(record),
            Err(DynamicObjectError::DuplicateObject)
        );
    }

    #[test]
    fn store_reuses_deleted_object_extent_before_tail() {
        let mut store = DynamicObjectStore::new(64, 8).unwrap();
        let first = store
            .create_object(TypedObjectRecord::new(
                ObjectId::new(0x8001),
                ObjectKind::ServiceMonitorWindow,
                1,
            ))
            .unwrap();
        let middle = store
            .create_object(TypedObjectRecord::new(
                ObjectId::new(0x8002),
                ObjectKind::PythonConsoleWindow,
                1,
            ))
            .unwrap();
        let tail = store
            .create_object(TypedObjectRecord::new(
                ObjectId::new(0x8003),
                ObjectKind::SettingsPanelWindow,
                1,
            ))
            .unwrap();

        store.delete_object(ObjectId::new(0x8002)).unwrap();
        let replacement = store
            .create_object(TypedObjectRecord::new(
                ObjectId::new(0x8004),
                ObjectKind::ApplicationLauncherWindow,
                1,
            ))
            .unwrap();

        assert_eq!(first.start_block(), 0);
        assert_eq!(middle.start_block(), 1);
        assert_eq!(tail.start_block(), 2);
        assert_eq!(replacement.start_block(), middle.start_block());
        assert_eq!(store.object_count(), 3);
    }

    #[test]
    fn store_returns_existing_typed_object_by_id() {
        let mut store = DynamicObjectStore::new(64, 12).unwrap();
        let record = TypedObjectRecord::new(ObjectId::new(1042), ObjectKind::Note, 1);

        store.create_object(record).unwrap();

        assert_eq!(store.object(ObjectId::new(1042)), Some(record));
        assert_eq!(store.object(ObjectId::new(2001)), None);
    }

    #[test]
    fn store_restores_records_without_losing_allocated_extents() {
        let first = DynamicObjectRecord {
            object: TypedObjectRecord::new(ObjectId::new(1042), ObjectKind::Note, 1),
            extent: BlockExtent::new(3, 1),
        };
        let second = DynamicObjectRecord {
            object: TypedObjectRecord::new(ObjectId::new(1043), ObjectKind::Note, 1),
            extent: BlockExtent::new(7, 1),
        };
        let mut records = [None; MAX_DYNAMIC_OBJECTS];
        records[0] = Some(first);
        records[1] = Some(second);

        let restored =
            DynamicObjectStore::restore_from_records(64, 12, (1 << 3) | (1 << 7), records).unwrap();

        assert_eq!(restored.object(ObjectId::new(1042)), Some(first.object));
        assert_eq!(restored.extent_for(ObjectId::new(1042)), Some(first.extent));
        assert_eq!(
            restored.extent_for(ObjectId::new(1043)),
            Some(second.extent)
        );
        assert_eq!(restored.allocator_bitmap(), (1 << 3) | (1 << 7));
    }

    #[test]
    fn store_capacity_keeps_eight_shell_notes_plus_external_denial_fixture() {
        let mut store = DynamicObjectStore::new(64, 12).unwrap();
        store
            .create_object(TypedObjectRecord::new(
                ObjectId::new(2001),
                ObjectKind::Note,
                1,
            ))
            .unwrap();

        for index in 0..pythos_shared::object_shell_abi::MAX_QUERY_RESULTS {
            store
                .create_object(TypedObjectRecord::new(
                    ObjectId::new(1042 + index as u64),
                    ObjectKind::Note,
                    1,
                ))
                .unwrap();
        }

        assert_eq!(
            store.object_count(),
            pythos_shared::object_shell_abi::MAX_QUERY_RESULTS + 1
        );
    }
}
