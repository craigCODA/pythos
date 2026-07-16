//! Typed, queryable relationships between ADR 0022 object records.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::shell_objects::{ObjectId, ObjectKind};
use crate::typed_object_format::TypedObjectRecord;

const MAX_OBJECTS: usize = 4;
const MAX_RELATIONSHIPS: usize = 8;

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
pub struct RelationshipStore {
    objects: [Option<TypedObjectRecord>; MAX_OBJECTS],
    relationships: [Option<ObjectRelationship>; MAX_RELATIONSHIPS],
}

impl RelationshipStore {
    pub const fn new() -> Self {
        Self {
            objects: [None; MAX_OBJECTS],
            relationships: [None; MAX_RELATIONSHIPS],
        }
    }

    pub fn insert_object(&mut self, object: TypedObjectRecord) -> Result<(), RelationshipError> {
        if self.contains_object(object.object_id()) {
            return Err(RelationshipError::DuplicateObject);
        }
        let mut index = 0;
        while index < MAX_OBJECTS {
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
        while index < MAX_RELATIONSHIPS {
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
        while index < MAX_RELATIONSHIPS {
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
        while index < MAX_RELATIONSHIPS {
            if self.relationships[index].is_some() {
                count += 1;
            }
            index += 1;
        }
        count
    }

    fn contains_object(self, object_id: ObjectId) -> bool {
        let mut index = 0;
        while index < MAX_OBJECTS {
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
        while index < MAX_RELATIONSHIPS {
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
}
