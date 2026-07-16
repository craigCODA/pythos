//! Minimal Phase 7 object browser over typed objects, relationships, and revisions.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::{
    object_relationships::{
        ObjectRelationship, RelationshipError, RelationshipKind, RelationshipStore,
    },
    revision_history::{RevisionHistory, RevisionHistoryError},
    service_identity::{ServiceIdentityError, ServiceIdentityTable},
    shell_apps::{ShellAppError, register_first_party_apps},
    shell_objects::{DrawableObject, ObjectId, ObjectKind, PresentationBinding, ShellObjectError},
    tasks::TaskId,
    typed_object_format::{ObjectFormatError, TypedObjectRecord},
    workspace_objects::{
        WORKSPACE_SESSION_OBJECT_ID, WorkspaceObjectError, workspace_session_record,
    },
};

const MAX_BROWSER_OBJECTS: usize = 6;
const OBJECT_BROWSER_WINDOW_ID: ObjectId = ObjectId::new(0x7501);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectBrowserError {
    ObjectTableFull,
    DuplicateObject,
    MissingObject,
    Relationship(RelationshipError),
    RevisionHistory(RevisionHistoryError),
    ServiceIdentity(ServiceIdentityError),
    ShellApp(ShellAppError),
    ShellObject(ShellObjectError),
    ObjectFormat(ObjectFormatError),
    WorkspaceObject(WorkspaceObjectError),
}

impl From<RelationshipError> for ObjectBrowserError {
    fn from(error: RelationshipError) -> Self {
        Self::Relationship(error)
    }
}

impl From<RevisionHistoryError> for ObjectBrowserError {
    fn from(error: RevisionHistoryError) -> Self {
        Self::RevisionHistory(error)
    }
}

impl From<ServiceIdentityError> for ObjectBrowserError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::ServiceIdentity(error)
    }
}

impl From<ShellAppError> for ObjectBrowserError {
    fn from(error: ShellAppError) -> Self {
        Self::ShellApp(error)
    }
}

impl From<ShellObjectError> for ObjectBrowserError {
    fn from(error: ShellObjectError) -> Self {
        Self::ShellObject(error)
    }
}

impl From<ObjectFormatError> for ObjectBrowserError {
    fn from(error: ObjectFormatError) -> Self {
        Self::ObjectFormat(error)
    }
}

impl From<WorkspaceObjectError> for ObjectBrowserError {
    fn from(error: WorkspaceObjectError) -> Self {
        Self::WorkspaceObject(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectBrowserApp {
    window: DrawableObject,
    binding: PresentationBinding,
}

impl ObjectBrowserApp {
    pub fn new() -> Result<Self, ObjectBrowserError> {
        let window = DrawableObject::new(OBJECT_BROWSER_WINDOW_ID, ObjectKind::ObjectBrowserWindow);
        let binding = PresentationBinding::new(window, 0, 0, 8, 4, 4)?;
        Ok(Self { window, binding })
    }

    pub const fn window(self) -> DrawableObject {
        self.window
    }

    pub const fn binding(self) -> PresentationBinding {
        self.binding
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserEntry {
    object_id: ObjectId,
    object_kind: ObjectKind,
}

impl BrowserEntry {
    pub const fn object_id(self) -> ObjectId {
        self.object_id
    }

    pub const fn object_kind(self) -> ObjectKind {
        self.object_kind
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserDetail {
    entry: BrowserEntry,
    relationship_target: Option<ObjectId>,
    prior_revision_count: usize,
}

impl BrowserDetail {
    pub const fn entry(self) -> BrowserEntry {
        self.entry
    }

    pub const fn relationship_target(self) -> Option<ObjectId> {
        self.relationship_target
    }

    pub const fn prior_revision_count(self) -> usize {
        self.prior_revision_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectBrowserStore {
    objects: [Option<TypedObjectRecord>; MAX_BROWSER_OBJECTS],
    relationships: RelationshipStore,
    revisions: RevisionHistory,
}

impl ObjectBrowserStore {
    pub const fn new() -> Self {
        Self {
            objects: [None; MAX_BROWSER_OBJECTS],
            relationships: RelationshipStore::new(),
            revisions: RevisionHistory::new(),
        }
    }

    pub fn add_object(&mut self, object: TypedObjectRecord) -> Result<(), ObjectBrowserError> {
        if self.find_object(object.object_id()).is_some() {
            return Err(ObjectBrowserError::DuplicateObject);
        }
        let mut index = 0;
        while index < MAX_BROWSER_OBJECTS {
            if self.objects[index].is_none() {
                self.objects[index] = Some(object);
                return Ok(());
            }
            index += 1;
        }
        Err(ObjectBrowserError::ObjectTableFull)
    }

    pub fn add_relationship(
        &mut self,
        relationship: ObjectRelationship,
    ) -> Result<(), ObjectBrowserError> {
        self.relationships.add_relationship(relationship)?;
        Ok(())
    }

    pub fn relationship_store_mut(&mut self) -> &mut RelationshipStore {
        &mut self.relationships
    }

    pub fn revision_history_mut(&mut self) -> &mut RevisionHistory {
        &mut self.revisions
    }

    pub fn object_count(self) -> usize {
        let mut count = 0;
        let mut index = 0;
        while index < MAX_BROWSER_OBJECTS {
            if self.objects[index].is_some() {
                count += 1;
            }
            index += 1;
        }
        count
    }

    pub fn list_entry(self, index: usize) -> Option<BrowserEntry> {
        self.objects
            .get(index)
            .copied()
            .flatten()
            .map(|object| BrowserEntry {
                object_id: object.object_id(),
                object_kind: object.object_kind(),
            })
    }

    pub fn inspect(
        self,
        object_id: ObjectId,
        relationship_kind: RelationshipKind,
    ) -> Result<BrowserDetail, ObjectBrowserError> {
        let object = self
            .find_object(object_id)
            .ok_or(ObjectBrowserError::MissingObject)?;
        let relationship_target = self
            .relationships
            .query_first(object_id, relationship_kind)
            .map(|relationship| relationship.target());
        Ok(BrowserDetail {
            entry: BrowserEntry {
                object_id,
                object_kind: object.object_kind(),
            },
            relationship_target,
            prior_revision_count: self.revisions.revision_count(object_id),
        })
    }

    fn find_object(self, object_id: ObjectId) -> Option<TypedObjectRecord> {
        let mut index = 0;
        while index < MAX_BROWSER_OBJECTS {
            if let Some(object) = self.objects[index]
                && object.object_id() == object_id
            {
                return Some(object);
            }
            index += 1;
        }
        None
    }
}

pub fn run_self_test() -> Result<(), ObjectBrowserError> {
    let app = ObjectBrowserApp::new()?;
    if app.window().kind() != ObjectKind::ObjectBrowserWindow || app.binding().z_order() != 4 {
        return Err(ObjectBrowserError::MissingObject);
    }

    let store = sample_store()?;
    let first_entry = store
        .list_entry(0)
        .ok_or(ObjectBrowserError::MissingObject)?;
    if store.object_count() != 3
        || first_entry.object_id() != WORKSPACE_SESSION_OBJECT_ID
        || first_entry.object_kind() != ObjectKind::WorkspaceSession
    {
        return Err(ObjectBrowserError::MissingObject);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT_BROWSER:LIST");

    let service_detail = store.inspect(ObjectId::new(0x7202), RelationshipKind::Blocks)?;
    let workspace_detail =
        store.inspect(WORKSPACE_SESSION_OBJECT_ID, RelationshipKind::DependsOn)?;
    if service_detail.relationship_target() != Some(ObjectId::new(0x7204))
        || workspace_detail.prior_revision_count() != 1
        || workspace_detail.entry().object_kind() != ObjectKind::WorkspaceSession
    {
        return Err(ObjectBrowserError::MissingObject);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT_BROWSER:DETAIL");
    Ok(())
}

fn sample_store() -> Result<ObjectBrowserStore, ObjectBrowserError> {
    let apps = register_first_party_apps()?;
    let workspace = workspace_session_record(&apps)?;
    let service_monitor =
        TypedObjectRecord::new(ObjectId::new(0x7202), ObjectKind::ServiceMonitorWindow, 1);
    let settings_panel =
        TypedObjectRecord::new(ObjectId::new(0x7204), ObjectKind::SettingsPanelWindow, 1);
    let mut store = ObjectBrowserStore::new();
    store.add_object(workspace)?;
    store.add_object(service_monitor)?;
    store.add_object(settings_panel)?;
    store.relationship_store_mut().insert_object(workspace)?;
    store
        .relationship_store_mut()
        .insert_object(service_monitor)?;
    store
        .relationship_store_mut()
        .insert_object(settings_panel)?;
    store.add_relationship(ObjectRelationship::new(
        service_monitor.object_id(),
        RelationshipKind::Blocks,
        settings_panel.object_id(),
    ))?;

    let mut identities = ServiceIdentityTable::new();
    let writer = identities.register_task(TaskId::new(95))?;
    store
        .revision_history_mut()
        .create_object(workspace, 200, writer)?;
    store
        .revision_history_mut()
        .update_object(workspace, 220, writer)?;
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_browser_lists_typed_objects() {
        let store = sample_store().unwrap();

        assert_eq!(store.object_count(), 3);
        assert_eq!(
            store.list_entry(0).unwrap(),
            BrowserEntry {
                object_id: WORKSPACE_SESSION_OBJECT_ID,
                object_kind: ObjectKind::WorkspaceSession,
            }
        );
    }

    #[test]
    fn object_browser_shows_relationship_target() {
        let store = sample_store().unwrap();

        let detail = store
            .inspect(ObjectId::new(0x7202), RelationshipKind::Blocks)
            .unwrap();

        assert_eq!(detail.relationship_target(), Some(ObjectId::new(0x7204)));
        assert_eq!(detail.prior_revision_count(), 0);
    }

    #[test]
    fn object_browser_shows_revision_count() {
        let store = sample_store().unwrap();

        let detail = store
            .inspect(WORKSPACE_SESSION_OBJECT_ID, RelationshipKind::Blocks)
            .unwrap();

        assert_eq!(detail.entry().object_kind(), ObjectKind::WorkspaceSession);
        assert_eq!(detail.prior_revision_count(), 1);
    }
}
