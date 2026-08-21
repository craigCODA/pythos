//! Typed, queryable relationships between ADR 0022 object records.
#![cfg_attr(test, allow(dead_code))]
// Some items are only used by the normal-boot object-service path (a
// `verify`-excluded set of modules), so they are legitimately unused under
// `--features verify`.
#![cfg_attr(feature = "verify", allow(dead_code))]

use crate::dynamic_object_store::MAX_DYNAMIC_OBJECTS;
#[cfg(not(test))]
use crate::serial;
use crate::shell_objects::{ObjectId, ObjectKind};
use crate::typed_object_format::TypedObjectRecord;

pub const SHELL_WORKSPACE_OBJECT_ID: u64 = 0x5059_5753_4845_4C01;
pub const EXTERNAL_WORKSPACE_OBJECT_ID: u64 = 0x5059_5753_4558_5401;
const LEGACY_RELATIONSHIP_OBJECTS: usize = 4;
const LEGACY_RELATIONSHIPS: usize = 8;
pub const OBJECT_SERVICE_RELATIONSHIP_OBJECTS: usize = MAX_DYNAMIC_OBJECTS + 2;
pub const OBJECT_SERVICE_RELATIONSHIPS: usize = MAX_DYNAMIC_OBJECTS;

pub type RelationshipStore =
    BoundedRelationshipStore<LEGACY_RELATIONSHIP_OBJECTS, LEGACY_RELATIONSHIPS>;
pub type ObjectServiceRelationshipStore =
    BoundedRelationshipStore<OBJECT_SERVICE_RELATIONSHIP_OBJECTS, OBJECT_SERVICE_RELATIONSHIPS>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelationshipError {
    ObjectTableFull,
    RelationshipTableFull,
    DuplicateObject,
    DuplicateRelationship,
    UnknownSource,
    UnknownTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelationshipKind {
    Blocks,
    CreatedBy,
    DependsOn,
    BelongsTo,
    NameBinding,
    BindingTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectRelationship {
    source: ObjectId,
    kind: RelationshipKind,
    target: ObjectId,
}

impl ObjectRelationship {
    pub const fn new(source: ObjectId, kind: RelationshipKind, target: ObjectId) -> Self {
        Self {
            source,
            kind,
            target,
        }
    }

    pub const fn source(self) -> ObjectId {
        self.source
    }

    pub const fn kind(self) -> RelationshipKind {
        self.kind
    }

    pub const fn target(self) -> ObjectId {
        self.target
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundedRelationshipStore<
    const OBJECT_CAPACITY: usize,
    const RELATIONSHIP_CAPACITY: usize,
> {
    objects: [Option<TypedObjectRecord>; OBJECT_CAPACITY],
    relationships: [Option<ObjectRelationship>; RELATIONSHIP_CAPACITY],
}

impl<const OBJECT_CAPACITY: usize, const RELATIONSHIP_CAPACITY: usize>
    BoundedRelationshipStore<OBJECT_CAPACITY, RELATIONSHIP_CAPACITY>
{
    pub const fn new() -> Self {
        Self {
            objects: [None; OBJECT_CAPACITY],
            relationships: [None; RELATIONSHIP_CAPACITY],
        }
    }

    pub fn insert_object(&mut self, object: TypedObjectRecord) -> Result<(), RelationshipError> {
        if self.contains_object(object.object_id()) {
            return Err(RelationshipError::DuplicateObject);
        }
        let mut index = 0;
        while index < OBJECT_CAPACITY {
            if self.objects[index].is_none() {
                self.objects[index] = Some(object);
                return Ok(());
            }
            index += 1;
        }
        Err(RelationshipError::ObjectTableFull)
    }

    pub fn add_relationship(
        &mut self,
        relationship: ObjectRelationship,
    ) -> Result<(), RelationshipError> {
        if !self.contains_object(relationship.source()) {
            return Err(RelationshipError::UnknownSource);
        }
        if !self.contains_object(relationship.target()) {
            return Err(RelationshipError::UnknownTarget);
        }
        if self.contains_relationship(relationship) {
            return Err(RelationshipError::DuplicateRelationship);
        }
        let mut index = 0;
        while index < RELATIONSHIP_CAPACITY {
            if self.relationships[index].is_none() {
                self.relationships[index] = Some(relationship);
                return Ok(());
            }
            index += 1;
        }
        Err(RelationshipError::RelationshipTableFull)
    }

    pub fn query_first(
        self,
        source: ObjectId,
        kind: RelationshipKind,
    ) -> Option<ObjectRelationship> {
        let mut index = 0;
        while index < RELATIONSHIP_CAPACITY {
            if let Some(relationship) = self.relationships[index]
                && relationship.source() == source
                && relationship.kind() == kind
            {
                return Some(relationship);
            }
            index += 1;
        }
        None
    }

    pub fn relationship_count(self) -> usize {
        let mut count = 0;
        let mut index = 0;
        while index < RELATIONSHIP_CAPACITY {
            if self.relationships[index].is_some() {
                count += 1;
            }
            index += 1;
        }
        count
    }

    pub fn has_object(self, object_id: ObjectId) -> bool {
        self.contains_object(object_id)
    }

    pub fn relationship_records(self) -> [Option<ObjectRelationship>; RELATIONSHIP_CAPACITY] {
        self.relationships
    }

    fn contains_object(self, object_id: ObjectId) -> bool {
        let mut index = 0;
        while index < OBJECT_CAPACITY {
            if let Some(object) = self.objects[index]
                && object.object_id() == object_id
            {
                return true;
            }
            index += 1;
        }
        false
    }

    fn contains_relationship(self, relationship: ObjectRelationship) -> bool {
        let mut index = 0;
        while index < RELATIONSHIP_CAPACITY {
            if self.relationships[index] == Some(relationship) {
                return true;
            }
            index += 1;
        }
        false
    }
}

pub fn run_self_test() -> Result<(), RelationshipError> {
    let service_monitor =
        TypedObjectRecord::new(ObjectId::new(0x7202), ObjectKind::ServiceMonitorWindow, 1);
    let python_console =
        TypedObjectRecord::new(ObjectId::new(0x7203), ObjectKind::PythonConsoleWindow, 1);
    let settings_panel =
        TypedObjectRecord::new(ObjectId::new(0x7204), ObjectKind::SettingsPanelWindow, 1);
    let mut store = RelationshipStore::new();
    store.insert_object(service_monitor)?;
    store.insert_object(python_console)?;
    store.insert_object(settings_panel)?;

    let blocks_settings = ObjectRelationship::new(
        service_monitor.object_id(),
        RelationshipKind::Blocks,
        settings_panel.object_id(),
    );
    store.add_relationship(blocks_settings)?;
    store.add_relationship(ObjectRelationship::new(
        python_console.object_id(),
        RelationshipKind::DependsOn,
        service_monitor.object_id(),
    ))?;
    store.add_relationship(ObjectRelationship::new(
        settings_panel.object_id(),
        RelationshipKind::CreatedBy,
        python_console.object_id(),
    ))?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT:RELATIONSHIP");

    let queried = store
        .query_first(service_monitor.object_id(), RelationshipKind::Blocks)
        .ok_or(RelationshipError::UnknownTarget)?;
    if queried.target().raw() != settings_panel.object_id().raw() || store.relationship_count() != 3
    {
        return Err(RelationshipError::UnknownTarget);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(raw: u64, kind: ObjectKind) -> TypedObjectRecord {
        TypedObjectRecord::new(ObjectId::new(raw), kind, 1)
    }

    #[test]
    fn relationship_requires_known_source_and_target_objects() {
        let source = object(1, ObjectKind::ServiceMonitorWindow);
        let target = object(2, ObjectKind::SettingsPanelWindow);
        let mut store = RelationshipStore::new();
        store.insert_object(source).unwrap();

        assert_eq!(
            store.add_relationship(ObjectRelationship::new(
                source.object_id(),
                RelationshipKind::Blocks,
                target.object_id(),
            )),
            Err(RelationshipError::UnknownTarget)
        );
    }

    #[test]
    fn typed_relationships_are_queryable_by_source_and_kind() {
        let source = object(1, ObjectKind::ServiceMonitorWindow);
        let target = object(2, ObjectKind::SettingsPanelWindow);
        let other = object(3, ObjectKind::PythonConsoleWindow);
        let mut store = RelationshipStore::new();
        store.insert_object(source).unwrap();
        store.insert_object(target).unwrap();
        store.insert_object(other).unwrap();

        store
            .add_relationship(ObjectRelationship::new(
                source.object_id(),
                RelationshipKind::Blocks,
                target.object_id(),
            ))
            .unwrap();
        store
            .add_relationship(ObjectRelationship::new(
                source.object_id(),
                RelationshipKind::CreatedBy,
                other.object_id(),
            ))
            .unwrap();

        assert_eq!(
            store
                .query_first(source.object_id(), RelationshipKind::Blocks)
                .unwrap()
                .target(),
            target.object_id()
        );
        assert_eq!(
            store.query_first(source.object_id(), RelationshipKind::DependsOn),
            None
        );
    }

    #[test]
    fn duplicate_relationships_are_rejected() {
        let source = object(1, ObjectKind::ServiceMonitorWindow);
        let target = object(2, ObjectKind::SettingsPanelWindow);
        let relationship = ObjectRelationship::new(
            source.object_id(),
            RelationshipKind::Blocks,
            target.object_id(),
        );
        let mut store = RelationshipStore::new();
        store.insert_object(source).unwrap();
        store.insert_object(target).unwrap();
        store.add_relationship(relationship).unwrap();

        assert_eq!(
            store.add_relationship(relationship),
            Err(RelationshipError::DuplicateRelationship)
        );
    }

    #[test]
    fn belongs_to_relationship_distinguishes_shell_and_external_workspaces() {
        let shell_workspace = object(SHELL_WORKSPACE_OBJECT_ID, ObjectKind::WorkspaceSession);
        let external_workspace = object(EXTERNAL_WORKSPACE_OBJECT_ID, ObjectKind::WorkspaceSession);
        let note = object(1042, ObjectKind::Note);
        let external_note = object(2001, ObjectKind::Note);
        let mut store = RelationshipStore::new();
        store.insert_object(shell_workspace).unwrap();
        store.insert_object(external_workspace).unwrap();
        store.insert_object(note).unwrap();
        store.insert_object(external_note).unwrap();

        store
            .add_relationship(ObjectRelationship::new(
                note.object_id(),
                RelationshipKind::BelongsTo,
                shell_workspace.object_id(),
            ))
            .unwrap();
        store
            .add_relationship(ObjectRelationship::new(
                external_note.object_id(),
                RelationshipKind::BelongsTo,
                external_workspace.object_id(),
            ))
            .unwrap();

        assert_eq!(
            store
                .query_first(note.object_id(), RelationshipKind::BelongsTo)
                .unwrap()
                .target(),
            shell_workspace.object_id()
        );
        assert_eq!(
            store
                .query_first(external_note.object_id(), RelationshipKind::BelongsTo)
                .unwrap()
                .target(),
            external_workspace.object_id()
        );
    }

    #[test]
    fn relationship_capacity_keeps_shell_notes_external_fixture_and_task_history() {
        let mut store = ObjectServiceRelationshipStore::new();
        let shell_workspace = object(SHELL_WORKSPACE_OBJECT_ID, ObjectKind::WorkspaceSession);
        let external_workspace = object(EXTERNAL_WORKSPACE_OBJECT_ID, ObjectKind::WorkspaceSession);
        store.insert_object(shell_workspace).unwrap();
        store.insert_object(external_workspace).unwrap();

        for index in 0..pythos_shared::object_shell_abi::MAX_QUERY_RESULTS {
            let note = object(1042 + index as u64, ObjectKind::Note);
            store.insert_object(note).unwrap();
            store
                .add_relationship(ObjectRelationship::new(
                    note.object_id(),
                    RelationshipKind::BelongsTo,
                    shell_workspace.object_id(),
                ))
                .unwrap();
        }

        let external_note = object(2001, ObjectKind::Note);
        store.insert_object(external_note).unwrap();
        store
            .add_relationship(ObjectRelationship::new(
                external_note.object_id(),
                RelationshipKind::BelongsTo,
                external_workspace.object_id(),
            ))
            .unwrap();

        let task_history_capacity = crate::dynamic_object_store::MAX_DYNAMIC_OBJECTS
            - (pythos_shared::object_shell_abi::MAX_QUERY_RESULTS + 1);
        for index in 0..task_history_capacity {
            let event = object(3000 + index as u64, ObjectKind::TaskEvent);
            store.insert_object(event).unwrap();
            store
                .add_relationship(ObjectRelationship::new(
                    event.object_id(),
                    RelationshipKind::BelongsTo,
                    shell_workspace.object_id(),
                ))
                .unwrap();
        }

        assert_eq!(
            store.relationship_count(),
            crate::dynamic_object_store::MAX_DYNAMIC_OBJECTS
        );
    }
}
