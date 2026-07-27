//! Retained object/capability service for ADR 0052.
#![cfg_attr(test, allow(dead_code))]

#[cfg(test)]
use crate::object_service_checkpoint::read_object_service_checkpoint;
use crate::{
    block_device::BlockDeviceInfo,
    capabilities::{CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask},
    dynamic_object_store::{
        DynamicObjectError, DynamicObjectRecord, DynamicObjectStore, MAX_DYNAMIC_OBJECTS,
    },
    object_relationships::{
        EXTERNAL_WORKSPACE_OBJECT_ID, ObjectRelationship, ObjectServiceRelationshipStore,
        RelationshipError, RelationshipKind, SHELL_WORKSPACE_OBJECT_ID,
    },
    object_service_checkpoint::{
        ObjectExtentRecord, ObjectServiceSnapshot, WorkspaceRelationshipRecord,
    },
    process_context::ActiveUserProcess,
    revision_history::{ObjectServiceRevisionHistory, RevisionHistoryError},
    service_identity::{ServiceId, ServiceIdentityError, ServiceIdentityTable},
    shell_objects::{ObjectId, ObjectKind},
    storage_allocator::BlockExtent,
    storage_quotas::{StorageQuotaError, StorageQuotaTable},
    tasks::TaskId,
    typed_object_format::{ObjectFormatError, TypedObjectField, TypedObjectRecord},
};
#[cfg(not(test))]
use core::mem::MaybeUninit;
use pythos_shared::{
    object_shell_abi::{MAX_QUERY_RESULTS, ObjectListEntry, PackedCapability},
    user_program_manifest::{INTRUDER_PRINCIPAL_ID, SHELL_PRINCIPAL_ID},
};

const OBJECT_SERVICE_BASE_SECTOR: u64 = 96;
const OBJECT_SERVICE_BLOCK_COUNT: u16 = 12;
const FIRST_SHELL_NOTE_ID: u64 = 1042;
const KNOWN_EXTERNAL_NOTE_ID: u64 = 2001;
const SHELL_TASK_ID: u64 = 180;
const INTRUDER_TASK_ID: u64 = 181;
const SHELL_PROGRAM_DIGEST: u64 = 0x0051_0052;
const INTRUDER_PROGRAM_DIGEST: u64 = 0xBAD0_0052;
const TEXT_FIELD_ID: u16 = pythos_shared::object_shell_abi::FIELD_TEXT;

const WORKSPACE_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ | RightsMask::WRITE);
const OBJECT_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ | RightsMask::WRITE);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectServiceError {
    Denied,
    NotFound,
    UnsupportedKind,
    ObjectFormat(ObjectFormatError),
    DynamicObject(DynamicObjectError),
    Relationship(RelationshipError),
    Revision(RevisionHistoryError),
    Capability(CapabilityError),
    Quota(StorageQuotaError),
    Identity(ServiceIdentityError),
    BadSnapshot,
}

impl From<ObjectFormatError> for ObjectServiceError {
    fn from(error: ObjectFormatError) -> Self {
        Self::ObjectFormat(error)
    }
}

impl From<DynamicObjectError> for ObjectServiceError {
    fn from(error: DynamicObjectError) -> Self {
        Self::DynamicObject(error)
    }
}

impl From<RelationshipError> for ObjectServiceError {
    fn from(error: RelationshipError) -> Self {
        Self::Relationship(error)
    }
}

impl From<RevisionHistoryError> for ObjectServiceError {
    fn from(error: RevisionHistoryError) -> Self {
        Self::Revision(error)
    }
}

impl From<CapabilityError> for ObjectServiceError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

impl From<StorageQuotaError> for ObjectServiceError {
    fn from(error: StorageQuotaError) -> Self {
        Self::Quota(error)
    }
}

impl From<ServiceIdentityError> for ObjectServiceError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::Identity(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectCreateResult {
    pub object_id: ObjectId,
    pub revision: u64,
    pub object_capability: PackedCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectInspection {
    pub object: TypedObjectRecord,
    pub revision: u64,
}

impl ObjectInspection {
    pub fn field_bytes(&self, field_id: u16) -> Option<[u8; 16]> {
        let mut index = 0usize;
        while index < self.object.field_count() {
            if let Some(field) = self.object.field(index)
                && field.field_id() == field_id
            {
                return Some(field.value());
            }
            index += 1;
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectService {
    objects: DynamicObjectStore,
    relationships: ObjectServiceRelationshipStore,
    revisions: ObjectServiceRevisionHistory,
    capabilities: CapabilityTable,
    quotas: StorageQuotaTable,
    shell_workspace: ResourceId,
    external_workspace: ResourceId,
    shell_caller: ActiveUserProcess,
    intruder_caller: ActiveUserProcess,
    shell_workspace_capability: PackedCapability,
    next_shell_note_id: u64,
    generation: u64,
    next_timestamp_ticks: u64,
}

impl ObjectService {
    #[cfg(not(test))]
    pub fn restore_or_initialize_in_place(
        slot: &mut MaybeUninit<Self>,
        device: BlockDeviceInfo,
    ) -> Result<(), ObjectServiceError> {
        crate::object_service_checkpoint::with_restored_object_service_checkpoint(
            device,
            |snapshot| match snapshot {
                Some(snapshot) => Self::write_decoded_snapshot_into(slot, snapshot),
                None => Self::write_seeded_into(slot).map(|_| ()),
            },
        )
        .map_err(|_| ObjectServiceError::BadSnapshot)?
    }

    #[cfg(not(test))]
    fn write_seeded_into(slot: &mut MaybeUninit<Self>) -> Result<&mut Self, ObjectServiceError> {
        let ptr = slot.as_mut_ptr();
        let invalid_process = ActiveUserProcess::new(ServiceId::invalid(), 0, 0);
        // SAFETY:
        // 1. Invariant: `ptr` points at the retained MaybeUninit<ObjectService>
        //    slot selected for normal boot initialization.
        // 2. Established by: retained_services passes its single static slot
        //    before marking the service usable by syscalls.
        // 3. Lifetime: each field write initializes storage that lives for the
        //    remainder of the boot.
        // 4. Pointer ownership: no ObjectService reference exists until all
        //    fields below have been written.
        // 5. Alignment: MaybeUninit<ObjectService> provides the alignment for
        //    every field of ObjectService.
        // 6. Mapped length: each addr_of_mut target names one field inside the
        //    ObjectService allocation.
        // 7. Concurrency: ADR 0051 normal boot initializes the service once on
        //    one CPU before shell launch and syscall dispatch.
        // 8. Violation: reading through `ptr` before all field writes complete
        //    would expose uninitialized service state.
        unsafe {
            core::ptr::addr_of_mut!((*ptr).objects).write(DynamicObjectStore::new(
                OBJECT_SERVICE_BASE_SECTOR,
                OBJECT_SERVICE_BLOCK_COUNT,
            )?);
            core::ptr::addr_of_mut!((*ptr).relationships)
                .write(ObjectServiceRelationshipStore::new());
            core::ptr::addr_of_mut!((*ptr).revisions).write(ObjectServiceRevisionHistory::new());
            core::ptr::addr_of_mut!((*ptr).capabilities).write(CapabilityTable::new());
            core::ptr::addr_of_mut!((*ptr).quotas).write(StorageQuotaTable::new());
            core::ptr::addr_of_mut!((*ptr).shell_workspace)
                .write(ResourceId::new(SHELL_WORKSPACE_OBJECT_ID));
            core::ptr::addr_of_mut!((*ptr).external_workspace)
                .write(ResourceId::new(EXTERNAL_WORKSPACE_OBJECT_ID));
            core::ptr::addr_of_mut!((*ptr).shell_caller).write(invalid_process);
            core::ptr::addr_of_mut!((*ptr).intruder_caller).write(invalid_process);
            core::ptr::addr_of_mut!((*ptr).shell_workspace_capability)
                .write(PackedCapability::from_raw(0));
            core::ptr::addr_of_mut!((*ptr).next_shell_note_id).write(FIRST_SHELL_NOTE_ID);
            core::ptr::addr_of_mut!((*ptr).generation).write(0);
            core::ptr::addr_of_mut!((*ptr).next_timestamp_ticks).write(1);
        }
        // SAFETY:
        // 1. Invariant: all ObjectService fields were initialized by the block
        //    immediately above.
        // 2. Established by: every field of the struct has exactly one write
        //    before this conversion.
        // 3. Lifetime: the returned borrow is tied to the caller's retained
        //    MaybeUninit slot.
        // 4. Pointer ownership: this function returns the only mutable borrow
        //    of the newly initialized service.
        // 5. Alignment: MaybeUninit<ObjectService> provided a properly aligned
        //    pointer.
        // 6. Mapped length: exactly one ObjectService object is referenced.
        // 7. Concurrency: normal boot service initialization is single-core and
        //    non-reentrant in this slice.
        // 8. Violation: a concurrent reference would break exclusive mutation
        //    during bootstrap seeding.
        let service = unsafe { &mut *ptr };
        service.initialize_runtime_authority()?;
        service.seed_workspace_roots()?;
        service.seed_external_note()?;
        Ok(service)
    }

    #[cfg(not(test))]
    fn write_decoded_snapshot_into(
        slot: &mut MaybeUninit<Self>,
        snapshot: &ObjectServiceSnapshot,
    ) -> Result<(), ObjectServiceError> {
        let service = Self::write_seeded_into(slot)?;
        service.objects = restore_dynamic_objects(snapshot)?;
        service.relationships = restore_relationships(snapshot)?;
        service.revisions = ObjectServiceRevisionHistory::restore_from_records(
            snapshot.current_revisions,
            snapshot.prior_revisions,
        )?;
        service.generation = snapshot.generation;
        service.next_shell_note_id = next_shell_note_id(snapshot);
        Ok(())
    }

    #[cfg(test)]
    pub fn restore_or_initialize(device: BlockDeviceInfo) -> Result<Self, ObjectServiceError> {
        match read_object_service_checkpoint(device).map_err(|_| ObjectServiceError::BadSnapshot)? {
            Some(snapshot) => Self::decode_snapshot(&snapshot),
            None => Self::new_seeded(),
        }
    }

    pub fn create_object(
        &mut self,
        caller: ActiveUserProcess,
        workspace: PackedCapability,
        kind: ObjectKind,
    ) -> Result<ObjectCreateResult, ObjectServiceError> {
        if kind != ObjectKind::Note {
            return Err(ObjectServiceError::UnsupportedKind);
        }
        self.validate_workspace(caller, workspace, RightsMask::new(RightsMask::WRITE))?;
        self.quotas.charge_blocks(caller.service_id(), 1)?;
        let object_id = ObjectId::new(self.next_shell_note_id);
        self.next_shell_note_id = self.next_shell_note_id.wrapping_add(1);
        let object = TypedObjectRecord::new(object_id, kind, 1);
        if let Err(error) = self.objects.create_object(object) {
            let _ = self.quotas.release_blocks(caller.service_id(), 1);
            return Err(error.into());
        }
        if let Err(error) = self.relationships.insert_object(object) {
            let _ = self.objects.delete_object(object_id);
            let _ = self.quotas.release_blocks(caller.service_id(), 1);
            return Err(error.into());
        }
        self.relationships
            .add_relationship(ObjectRelationship::new(
                object_id,
                RelationshipKind::BelongsTo,
                ObjectId::new(SHELL_WORKSPACE_OBJECT_ID),
            ))?;
        let timestamp = self.take_timestamp();
        self.revisions
            .create_object(object, timestamp, caller.service_id())?;
        let object_capability = self.grant_object_capability(caller, object_id)?;
        Ok(ObjectCreateResult {
            object_id,
            revision: 1,
            object_capability,
        })
    }

    pub fn query_objects(
        &mut self,
        caller: ActiveUserProcess,
        workspace: PackedCapability,
        kind: ObjectKind,
    ) -> Result<[ObjectListEntry; MAX_QUERY_RESULTS], ObjectServiceError> {
        self.validate_workspace(caller, workspace, RightsMask::new(RightsMask::READ))?;
        let mut results = [ObjectListEntry {
            object_id: 0,
            capability: PackedCapability::from_raw(0),
        }; MAX_QUERY_RESULTS];
        let records = self.objects.object_records();
        let mut result_index = 0;
        let mut index = 0;
        while index < MAX_DYNAMIC_OBJECTS && result_index < MAX_QUERY_RESULTS {
            if let Some(record) = records[index]
                && record.object.object_kind() == kind
                && self.object_belongs_to_workspace(
                    record.object.object_id(),
                    ObjectId::new(SHELL_WORKSPACE_OBJECT_ID),
                )
            {
                results[result_index] = ObjectListEntry {
                    object_id: record.object.object_id().raw(),
                    capability: self.grant_object_capability(caller, record.object.object_id())?,
                };
                result_index += 1;
            }
            index += 1;
        }
        Ok(results)
    }

    pub fn inspect_object(
        &self,
        caller: ActiveUserProcess,
        object_capability: PackedCapability,
        object_id: ObjectId,
    ) -> Result<ObjectInspection, ObjectServiceError> {
        self.validate_object(
            caller,
            object_capability,
            object_id,
            RightsMask::new(RightsMask::READ),
        )?;
        let current = self
            .revisions
            .current_revision(object_id)
            .ok_or(ObjectServiceError::NotFound)?;
        Ok(ObjectInspection {
            object: current.object(),
            revision: current.revision(),
        })
    }

    pub fn revise_field(
        &mut self,
        caller: ActiveUserProcess,
        object_capability: PackedCapability,
        object_id: ObjectId,
        field_id: u16,
        value: &[u8],
    ) -> Result<u64, ObjectServiceError> {
        self.validate_object(
            caller,
            object_capability,
            object_id,
            RightsMask::new(RightsMask::WRITE),
        )?;
        let current = self
            .revisions
            .current_revision(object_id)
            .ok_or(ObjectServiceError::NotFound)?;
        let next_revision = current.revision() + 1;
        let mut revised = TypedObjectRecord::new(
            object_id,
            current.object().object_kind(),
            next_revision as u16,
        );
        revised.push_field(TypedObjectField::new(
            field_id,
            next_revision as u16,
            value,
        )?)?;
        let mut staged_objects = self.objects;
        let mut staged_revisions = self.revisions;
        staged_objects.replace_object(revised)?;
        let timestamp = self.next_timestamp_ticks;
        let revision = staged_revisions.update_object(revised, timestamp, caller.service_id())?;
        self.objects = staged_objects;
        self.revisions = staged_revisions;
        self.next_timestamp_ticks = self.next_timestamp_ticks.wrapping_add(1);
        Ok(revision)
    }

    pub fn history(
        &self,
        caller: ActiveUserProcess,
        object_capability: PackedCapability,
        object_id: ObjectId,
    ) -> Result<u64, ObjectServiceError> {
        self.validate_object(
            caller,
            object_capability,
            object_id,
            RightsMask::new(RightsMask::READ),
        )?;
        let current = self
            .revisions
            .current_revision(object_id)
            .ok_or(ObjectServiceError::NotFound)?;
        Ok(current.revision())
    }

    fn new_seeded() -> Result<Self, ObjectServiceError> {
        let mut service = Self {
            objects: DynamicObjectStore::new(
                OBJECT_SERVICE_BASE_SECTOR,
                OBJECT_SERVICE_BLOCK_COUNT,
            )?,
            relationships: ObjectServiceRelationshipStore::new(),
            revisions: ObjectServiceRevisionHistory::new(),
            capabilities: CapabilityTable::new(),
            quotas: StorageQuotaTable::new(),
            shell_workspace: ResourceId::new(SHELL_WORKSPACE_OBJECT_ID),
            external_workspace: ResourceId::new(EXTERNAL_WORKSPACE_OBJECT_ID),
            shell_caller: ActiveUserProcess::new(ServiceId::invalid(), 0, 0),
            intruder_caller: ActiveUserProcess::new(ServiceId::invalid(), 0, 0),
            shell_workspace_capability: PackedCapability::from_raw(0),
            next_shell_note_id: FIRST_SHELL_NOTE_ID,
            generation: 0,
            next_timestamp_ticks: 1,
        };
        service.initialize_runtime_authority()?;
        service.seed_workspace_roots()?;
        service.seed_external_note()?;
        Ok(service)
    }

    fn initialize_runtime_authority(&mut self) -> Result<(), ObjectServiceError> {
        let mut identities = ServiceIdentityTable::new();
        let shell_service = identities.register_task(TaskId::new(SHELL_TASK_ID))?;
        let intruder_service = identities.register_task(TaskId::new(INTRUDER_TASK_ID))?;
        self.shell_caller =
            ActiveUserProcess::new(shell_service, SHELL_PRINCIPAL_ID, SHELL_PROGRAM_DIGEST);
        self.intruder_caller = ActiveUserProcess::new(
            intruder_service,
            INTRUDER_PRINCIPAL_ID,
            INTRUDER_PROGRAM_DIGEST,
        );
        self.capabilities = CapabilityTable::new();
        self.shell_workspace = ResourceId::new(SHELL_WORKSPACE_OBJECT_ID);
        self.external_workspace = ResourceId::new(EXTERNAL_WORKSPACE_OBJECT_ID);
        self.shell_workspace_capability = pack_capability(self.capabilities.grant(
            shell_service,
            self.shell_workspace,
            WORKSPACE_RIGHTS,
        )?);
        self.quotas = StorageQuotaTable::new();
        self.quotas
            .register(shell_service, MAX_QUERY_RESULTS as u64)?;
        self.quotas.register(intruder_service, 1)?;
        Ok(())
    }

    fn decode_snapshot(snapshot: &ObjectServiceSnapshot) -> Result<Self, ObjectServiceError> {
        let mut service = Self::new_seeded()?;
        service.objects = restore_dynamic_objects(snapshot)?;
        service.relationships = restore_relationships(snapshot)?;
        service.revisions = ObjectServiceRevisionHistory::restore_from_records(
            snapshot.current_revisions,
            snapshot.prior_revisions,
        )?;
        service.generation = snapshot.generation;
        service.next_shell_note_id = next_shell_note_id(snapshot);
        Ok(service)
    }

    fn encode_snapshot(&self) -> Result<ObjectServiceSnapshot, ObjectServiceError> {
        let object_records = self.objects.object_records();
        let mut objects = [None; crate::object_service_checkpoint::OBJECT_SERVICE_OBJECT_CAPACITY];
        let mut relationships = [None;
            crate::object_service_checkpoint::OBJECT_SERVICE_WORKSPACE_RELATIONSHIP_CAPACITY];
        let mut index = 0;
        while index < MAX_DYNAMIC_OBJECTS {
            if let Some(record) = object_records[index] {
                objects[index] = Some(ObjectExtentRecord {
                    extent_start: u64::from(record.extent.start_block()),
                    extent_len: record.extent.block_count(),
                    object: record.object,
                });
                if let Some(relationship) = self
                    .relationships
                    .query_first(record.object.object_id(), RelationshipKind::BelongsTo)
                {
                    relationships[index] = Some(WorkspaceRelationshipRecord {
                        object_id: relationship.source(),
                        workspace_id: relationship.target(),
                    });
                }
            }
            index += 1;
        }
        Ok(ObjectServiceSnapshot {
            generation: self.generation,
            allocated_bitmap: self.objects.allocator_bitmap(),
            objects,
            workspace_relationships: relationships,
            current_revisions: self.revisions.current_records(),
            prior_revisions: self.revisions.prior_records(),
        })
    }

    fn seed_workspace_roots(&mut self) -> Result<(), ObjectServiceError> {
        self.relationships.insert_object(TypedObjectRecord::new(
            ObjectId::new(SHELL_WORKSPACE_OBJECT_ID),
            ObjectKind::WorkspaceSession,
            1,
        ))?;
        self.relationships.insert_object(TypedObjectRecord::new(
            ObjectId::new(EXTERNAL_WORKSPACE_OBJECT_ID),
            ObjectKind::WorkspaceSession,
            1,
        ))?;
        Ok(())
    }

    fn seed_external_note(&mut self) -> Result<(), ObjectServiceError> {
        let mut external =
            TypedObjectRecord::new(ObjectId::new(KNOWN_EXTERNAL_NOTE_ID), ObjectKind::Note, 1);
        external.push_field(TypedObjectField::new(TEXT_FIELD_ID, 1, b"secret")?)?;
        self.quotas
            .charge_blocks(self.intruder_caller.service_id(), 1)?;
        self.objects.create_object(external)?;
        self.relationships.insert_object(external)?;
        self.relationships
            .add_relationship(ObjectRelationship::new(
                external.object_id(),
                RelationshipKind::BelongsTo,
                ObjectId::new(EXTERNAL_WORKSPACE_OBJECT_ID),
            ))?;
        let timestamp = self.take_timestamp();
        self.revisions
            .create_object(external, timestamp, self.intruder_caller.service_id())?;
        Ok(())
    }

    fn validate_workspace(
        &self,
        caller: ActiveUserProcess,
        capability: PackedCapability,
        rights: RightsMask,
    ) -> Result<(), ObjectServiceError> {
        self.capabilities
            .validate(
                caller.service_id(),
                unpack_capability(capability),
                self.shell_workspace,
                rights,
            )
            .map_err(|_| ObjectServiceError::Denied)
    }

    fn validate_object(
        &self,
        caller: ActiveUserProcess,
        capability: PackedCapability,
        object_id: ObjectId,
        rights: RightsMask,
    ) -> Result<(), ObjectServiceError> {
        self.capabilities
            .validate(
                caller.service_id(),
                unpack_capability(capability),
                ResourceId::new(object_id.raw()),
                rights,
            )
            .map_err(|_| ObjectServiceError::Denied)
    }

    fn grant_object_capability(
        &mut self,
        caller: ActiveUserProcess,
        object_id: ObjectId,
    ) -> Result<PackedCapability, ObjectServiceError> {
        Ok(pack_capability(self.capabilities.grant(
            caller.service_id(),
            ResourceId::new(object_id.raw()),
            OBJECT_RIGHTS,
        )?))
    }

    fn object_belongs_to_workspace(&self, object_id: ObjectId, workspace_id: ObjectId) -> bool {
        self.relationships
            .query_first(object_id, RelationshipKind::BelongsTo)
            .is_some_and(|relationship| relationship.target() == workspace_id)
    }

    fn take_timestamp(&mut self) -> u64 {
        let ticks = self.next_timestamp_ticks;
        self.next_timestamp_ticks = self.next_timestamp_ticks.wrapping_add(1);
        ticks
    }
}

#[cfg(test)]
impl ObjectService {
    pub fn new_for_test() -> Self {
        Self::new_seeded().unwrap()
    }

    pub fn test_shell_caller(&self) -> ActiveUserProcess {
        self.shell_caller
    }

    pub fn test_intruder_caller(&self) -> ActiveUserProcess {
        self.intruder_caller
    }

    pub fn test_shell_workspace_capability(&self) -> PackedCapability {
        self.shell_workspace_capability
    }

    pub fn object_exists_for_test(&self, object_id: ObjectId) -> bool {
        self.objects.object(object_id).is_some()
    }

    pub fn dynamic_object_for_test(&self, object_id: ObjectId) -> Option<TypedObjectRecord> {
        self.objects.object(object_id)
    }

    pub fn object_outside_shell_workspace_for_test(&self, object_id: ObjectId) -> bool {
        self.relationships
            .query_first(object_id, RelationshipKind::BelongsTo)
            .is_some_and(|relationship| relationship.target().raw() != SHELL_WORKSPACE_OBJECT_ID)
    }

    pub fn create_ungranted_note_for_test(
        &mut self,
        object_id: ObjectId,
        text: &[u8],
    ) -> Result<ObjectCreateResult, ObjectServiceError> {
        let mut object = TypedObjectRecord::new(object_id, ObjectKind::Note, 1);
        object.push_field(TypedObjectField::new(TEXT_FIELD_ID, 1, text)?)?;
        self.objects.create_object(object)?;
        self.relationships.insert_object(object)?;
        self.relationships
            .add_relationship(ObjectRelationship::new(
                object_id,
                RelationshipKind::BelongsTo,
                ObjectId::new(EXTERNAL_WORKSPACE_OBJECT_ID),
            ))?;
        let timestamp = self.take_timestamp();
        self.revisions
            .create_object(object, timestamp, self.intruder_caller.service_id())?;
        Ok(ObjectCreateResult {
            object_id,
            revision: 1,
            object_capability: PackedCapability::from_raw(0),
        })
    }

    pub fn encode_snapshot_for_test(&self) -> Result<ObjectServiceSnapshot, ObjectServiceError> {
        self.encode_snapshot()
    }

    pub fn decode_snapshot_for_test(
        snapshot: ObjectServiceSnapshot,
    ) -> Result<Self, ObjectServiceError> {
        Self::decode_snapshot(&snapshot)
    }

    pub fn object_capability_for_test(
        &mut self,
        caller: ActiveUserProcess,
        object_id: ObjectId,
    ) -> Result<PackedCapability, ObjectServiceError> {
        self.grant_object_capability(caller, object_id)
    }

    pub fn revoke_object_capability_for_test(
        &mut self,
        _caller: ActiveUserProcess,
        capability: PackedCapability,
    ) -> Result<(), ObjectServiceError> {
        self.capabilities
            .revoke(unpack_capability(capability))
            .map_err(|_| ObjectServiceError::Denied)
    }
}

fn restore_dynamic_objects(
    snapshot: &ObjectServiceSnapshot,
) -> Result<DynamicObjectStore, ObjectServiceError> {
    let mut records = [None; MAX_DYNAMIC_OBJECTS];
    let mut index = 0;
    while index < MAX_DYNAMIC_OBJECTS {
        if let Some(record) = snapshot.objects[index] {
            let start =
                u16::try_from(record.extent_start).map_err(|_| ObjectServiceError::BadSnapshot)?;
            records[index] = Some(DynamicObjectRecord {
                object: record.object,
                extent: BlockExtent::new(start, record.extent_len),
            });
        }
        index += 1;
    }
    Ok(DynamicObjectStore::restore_from_records(
        OBJECT_SERVICE_BASE_SECTOR,
        OBJECT_SERVICE_BLOCK_COUNT,
        snapshot.allocated_bitmap,
        records,
    )?)
}

fn restore_relationships(
    snapshot: &ObjectServiceSnapshot,
) -> Result<ObjectServiceRelationshipStore, ObjectServiceError> {
    let mut relationships = ObjectServiceRelationshipStore::new();
    relationships.insert_object(TypedObjectRecord::new(
        ObjectId::new(SHELL_WORKSPACE_OBJECT_ID),
        ObjectKind::WorkspaceSession,
        1,
    ))?;
    relationships.insert_object(TypedObjectRecord::new(
        ObjectId::new(EXTERNAL_WORKSPACE_OBJECT_ID),
        ObjectKind::WorkspaceSession,
        1,
    ))?;
    let mut object_index = 0;
    while object_index < MAX_DYNAMIC_OBJECTS {
        if let Some(record) = snapshot.objects[object_index] {
            relationships.insert_object(record.object)?;
        }
        object_index += 1;
    }
    let mut relationship_index = 0;
    while relationship_index < MAX_DYNAMIC_OBJECTS {
        if let Some(record) = snapshot.workspace_relationships[relationship_index] {
            if !relationships.has_object(record.object_id)
                || !relationships.has_object(record.workspace_id)
            {
                return Err(ObjectServiceError::BadSnapshot);
            }
            relationships.add_relationship(ObjectRelationship::new(
                record.object_id,
                RelationshipKind::BelongsTo,
                record.workspace_id,
            ))?;
        }
        relationship_index += 1;
    }
    Ok(relationships)
}

fn next_shell_note_id(snapshot: &ObjectServiceSnapshot) -> u64 {
    let mut next = FIRST_SHELL_NOTE_ID;
    let mut index = 0;
    while index < MAX_DYNAMIC_OBJECTS {
        if let Some(record) = snapshot.objects[index]
            && record.object.object_id().raw() >= FIRST_SHELL_NOTE_ID
            && record.object.object_id().raw() < KNOWN_EXTERNAL_NOTE_ID
            && record.object.object_id().raw() >= next
        {
            next = record.object.object_id().raw().wrapping_add(1);
        }
        index += 1;
    }
    next
}

fn pack_capability(handle: CapabilityHandle) -> PackedCapability {
    PackedCapability::from_parts(handle.slot(), handle.generation())
}

fn unpack_capability(capability: PackedCapability) -> CapabilityHandle {
    CapabilityHandle::from_parts(capability.slot(), capability.generation())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::object_shell_abi::MAX_QUERY_RESULTS;

    #[test]
    fn service_uses_workspace_and_object_capabilities_for_note_flow() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();

        let created = service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();
        assert_eq!(created.object_id, ObjectId::new(1042));
        assert_eq!(created.revision, 1);

        service
            .revise_field(
                shell,
                created.object_capability,
                ObjectId::new(1042),
                1,
                b"hello",
            )
            .unwrap();
        let inspected = service
            .inspect_object(shell, created.object_capability, ObjectId::new(1042))
            .unwrap();

        assert_eq!(inspected.revision, 2);
        assert_eq!(
            inspected.field_bytes(1),
            Some(*b"hello\0\0\0\0\0\0\0\0\0\0\0")
        );
    }

    #[test]
    fn known_object_id_without_object_capability_is_denied() {
        let service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();

        assert!(service.object_exists_for_test(ObjectId::new(2001)));
        assert!(service.object_outside_shell_workspace_for_test(ObjectId::new(2001)));
        assert_eq!(
            service.inspect_object(
                shell,
                service.test_shell_workspace_capability(),
                ObjectId::new(2001)
            ),
            Err(ObjectServiceError::Denied)
        );
    }

    #[test]
    fn query_returns_rebound_object_capabilities() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();

        let results = service
            .query_objects(shell, workspace, ObjectKind::Note)
            .unwrap();

        assert_eq!(results[0].object_id, created.object_id.raw());
        assert_eq!(
            service
                .inspect_object(shell, results[0].capability, created.object_id)
                .unwrap()
                .revision,
            1
        );
    }

    #[test]
    fn query_capacity_keeps_eight_shell_notes_plus_external_denial_fixture() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();

        for _ in 0..MAX_QUERY_RESULTS {
            service
                .create_object(shell, workspace, ObjectKind::Note)
                .unwrap();
        }

        let results = service
            .query_objects(shell, workspace, ObjectKind::Note)
            .unwrap();

        assert_eq!(
            results.iter().filter(|entry| entry.object_id != 0).count(),
            MAX_QUERY_RESULTS
        );
        assert!(results.iter().all(|entry| entry.object_id != 2001));
    }

    #[test]
    fn snapshot_does_not_serialize_runtime_capability_handles() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();
        let snapshot = service.encode_snapshot_for_test().unwrap();

        assert!(!snapshot.contains_runtime_handle_for_test(created.object_capability));
    }

    #[test]
    fn restored_shell_regains_authority_by_workspace_query() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();
        let snapshot = service.encode_snapshot_for_test().unwrap();

        let mut restored = ObjectService::decode_snapshot_for_test(snapshot).unwrap();
        let restored_shell = restored.test_shell_caller();
        let restored_workspace = restored.test_shell_workspace_capability();
        let restored_entries = restored
            .query_objects(restored_shell, restored_workspace, ObjectKind::Note)
            .unwrap();
        let restored_cap = restored_entries[0].capability;

        assert_eq!(restored_entries[0].object_id, created.object_id.raw());
        assert!(
            restored
                .inspect_object(restored_shell, restored_cap, ObjectId::new(1042))
                .is_ok()
        );
    }

    #[test]
    fn capability_handles_remain_holder_bound_and_revocable_within_one_boot() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let intruder = service.test_intruder_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();

        assert_eq!(
            service.inspect_object(intruder, created.object_capability, ObjectId::new(1042)),
            Err(ObjectServiceError::Denied)
        );
        service
            .revoke_object_capability_for_test(shell, created.object_capability)
            .unwrap();
        assert_eq!(
            service.inspect_object(shell, created.object_capability, ObjectId::new(1042)),
            Err(ObjectServiceError::Denied)
        );
    }

    #[test]
    fn repeated_queries_reuse_existing_object_capabilities() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();

        for _ in 0..MAX_QUERY_RESULTS {
            service
                .create_object(shell, workspace, ObjectKind::Note)
                .unwrap();
        }

        let mut expected = None;
        for _ in 0..6 {
            let results = service
                .query_objects(shell, workspace, ObjectKind::Note)
                .unwrap();
            assert_eq!(
                results.iter().filter(|entry| entry.object_id != 0).count(),
                MAX_QUERY_RESULTS
            );
            if let Some(previous) = expected {
                assert_eq!(results, previous);
            }
            expected = Some(results);
        }
    }

    #[test]
    fn failed_revision_update_does_not_mutate_dynamic_object() {
        let mut service = ObjectService::new_for_test();
        let shell = service.test_shell_caller();
        let workspace = service.test_shell_workspace_capability();
        let created = service
            .create_object(shell, workspace, ObjectKind::Note)
            .unwrap();

        for revision in 0..MAX_QUERY_RESULTS {
            let text = [b'a' + revision as u8];
            service
                .revise_field(
                    shell,
                    created.object_capability,
                    created.object_id,
                    TEXT_FIELD_ID,
                    &text,
                )
                .unwrap();
        }
        let before = service.dynamic_object_for_test(created.object_id).unwrap();

        assert_eq!(
            service.revise_field(
                shell,
                created.object_capability,
                created.object_id,
                TEXT_FIELD_ID,
                b"overflow",
            ),
            Err(ObjectServiceError::Revision(
                RevisionHistoryError::RevisionTableFull
            ))
        );

        let after = service.dynamic_object_for_test(created.object_id).unwrap();
        assert_eq!(after, before);
    }
}
