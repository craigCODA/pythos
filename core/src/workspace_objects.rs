//! Phase 7 workspace session objects built on ADR 0022 records.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::{
    revision_history::{RevisionHistory, RevisionHistoryError},
    service_identity::{ServiceIdentityError, ServiceIdentityTable},
    shell_apps::{
        ShellAppError, ShellAppKind, ShellApplication, ShellApplicationSet,
        register_first_party_apps,
    },
    shell_objects::{ObjectId, ObjectKind},
    tasks::TaskId,
    typed_object_format::{ObjectFormatError, TypedObjectField, TypedObjectRecord},
};

pub const WORKSPACE_SESSION_OBJECT_ID: ObjectId = ObjectId::new(0x7401);

const WORKSPACE_SCHEMA_VERSION: u16 = 1;
const FIELD_LAUNCHER_LAYOUT: u16 = 0x100;
const FIELD_SERVICE_MONITOR_LAYOUT: u16 = 0x101;
const FIELD_PYTHON_CONSOLE_LAYOUT: u16 = 0x102;
const FIELD_SETTINGS_PANEL_LAYOUT: u16 = 0x103;
const LAYOUT_FIELD_LEN: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceObjectError {
    MissingWindow,
    BadWorkspaceObject,
    LayoutOutOfRange,
    ReservedByteNonZero,
    ObjectFormat(ObjectFormatError),
    RevisionHistory(RevisionHistoryError),
    ServiceIdentity(ServiceIdentityError),
    ShellApp(ShellAppError),
}

impl From<ObjectFormatError> for WorkspaceObjectError {
    fn from(error: ObjectFormatError) -> Self {
        Self::ObjectFormat(error)
    }
}

impl From<RevisionHistoryError> for WorkspaceObjectError {
    fn from(error: RevisionHistoryError) -> Self {
        Self::RevisionHistory(error)
    }
}

impl From<ServiceIdentityError> for WorkspaceObjectError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::ServiceIdentity(error)
    }
}

impl From<ShellAppError> for WorkspaceObjectError {
    fn from(error: ShellAppError) -> Self {
        Self::ShellApp(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceWindowLayout {
    object_id: ObjectId,
    x: u16,
    y: u16,
    width: u8,
    height: u8,
    z_order: u8,
}

impl WorkspaceWindowLayout {
    pub fn from_app(app: ShellApplication) -> Result<Self, WorkspaceObjectError> {
        let binding = app.binding();
        if binding.x() > u16::MAX as usize
            || binding.y() > u16::MAX as usize
            || binding.width() > u8::MAX as usize
            || binding.height() > u8::MAX as usize
        {
            return Err(WorkspaceObjectError::LayoutOutOfRange);
        }
        Ok(Self {
            object_id: app.window().id(),
            x: binding.x() as u16,
            y: binding.y() as u16,
            width: binding.width() as u8,
            height: binding.height() as u8,
            z_order: binding.z_order(),
        })
    }

    pub const fn object_id(self) -> ObjectId {
        self.object_id
    }

    pub const fn x(self) -> u16 {
        self.x
    }

    pub const fn y(self) -> u16 {
        self.y
    }

    pub const fn width(self) -> u8 {
        self.width
    }

    pub const fn height(self) -> u8 {
        self.height
    }

    pub const fn z_order(self) -> u8 {
        self.z_order
    }

    pub fn encode(self) -> [u8; LAYOUT_FIELD_LEN] {
        let mut bytes = [0; LAYOUT_FIELD_LEN];
        write_u64(&mut bytes, 0, self.object_id.raw());
        write_u16(&mut bytes, 8, self.x);
        write_u16(&mut bytes, 10, self.y);
        bytes[12] = self.width;
        bytes[13] = self.height;
        bytes[14] = self.z_order;
        bytes[15] = 0;
        bytes
    }

    pub fn decode(field: TypedObjectField) -> Result<Self, WorkspaceObjectError> {
        if field.value_len() != LAYOUT_FIELD_LEN as u16 {
            return Err(WorkspaceObjectError::BadWorkspaceObject);
        }
        let bytes = field.value();
        if bytes[15] != 0 {
            return Err(WorkspaceObjectError::ReservedByteNonZero);
        }
        Ok(Self {
            object_id: ObjectId::new(read_u64(&bytes, 0)),
            x: read_u16(&bytes, 8),
            y: read_u16(&bytes, 10),
            width: bytes[12],
            height: bytes[13],
            z_order: bytes[14],
        })
    }
}

pub fn workspace_session_record(
    apps: &ShellApplicationSet,
) -> Result<TypedObjectRecord, WorkspaceObjectError> {
    let mut record = TypedObjectRecord::new(
        WORKSPACE_SESSION_OBJECT_ID,
        ObjectKind::WorkspaceSession,
        WORKSPACE_SCHEMA_VERSION,
    );
    push_layout_field(
        &mut record,
        FIELD_LAUNCHER_LAYOUT,
        apps.app(ShellAppKind::Launcher)
            .ok_or(WorkspaceObjectError::MissingWindow)?,
    )?;
    push_layout_field(
        &mut record,
        FIELD_SERVICE_MONITOR_LAYOUT,
        apps.app(ShellAppKind::ServiceMonitor)
            .ok_or(WorkspaceObjectError::MissingWindow)?,
    )?;
    push_layout_field(
        &mut record,
        FIELD_PYTHON_CONSOLE_LAYOUT,
        apps.app(ShellAppKind::PythonConsole)
            .ok_or(WorkspaceObjectError::MissingWindow)?,
    )?;
    push_layout_field(
        &mut record,
        FIELD_SETTINGS_PANEL_LAYOUT,
        apps.app(ShellAppKind::SettingsPanel)
            .ok_or(WorkspaceObjectError::MissingWindow)?,
    )?;
    Ok(record)
}

pub fn layout_for(
    record: TypedObjectRecord,
    object_id: ObjectId,
) -> Result<WorkspaceWindowLayout, WorkspaceObjectError> {
    if record.object_kind() != ObjectKind::WorkspaceSession {
        return Err(WorkspaceObjectError::BadWorkspaceObject);
    }
    let mut index = 0;
    while index < record.field_count() {
        let field = record
            .field(index)
            .ok_or(WorkspaceObjectError::BadWorkspaceObject)?;
        let layout = WorkspaceWindowLayout::decode(field)?;
        if layout.object_id() == object_id {
            return Ok(layout);
        }
        index += 1;
    }
    Err(WorkspaceObjectError::MissingWindow)
}

fn push_layout_field(
    record: &mut TypedObjectRecord,
    field_id: u16,
    app: ShellApplication,
) -> Result<(), WorkspaceObjectError> {
    let layout = WorkspaceWindowLayout::from_app(app)?;
    let encoded = layout.encode();
    record.push_field(TypedObjectField::new(field_id, 1, &encoded)?)?;
    Ok(())
}

pub fn run_self_test() -> Result<(), WorkspaceObjectError> {
    let apps = register_first_party_apps()?;
    let record = workspace_session_record(&apps)?;
    if record.object_id() != WORKSPACE_SESSION_OBJECT_ID
        || record.object_kind() != ObjectKind::WorkspaceSession
        || record.schema_version() != WORKSPACE_SCHEMA_VERSION
        || record.field_count() != 4
    {
        return Err(WorkspaceObjectError::BadWorkspaceObject);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:WORKSPACE:SESSION_OBJECT");

    let mut identities = ServiceIdentityTable::new();
    let writer = identities.register_task(TaskId::new(90))?;
    let mut history = RevisionHistory::new();
    history.create_object(record, 200, writer)?;
    let restored = history
        .current_revision(WORKSPACE_SESSION_OBJECT_ID)
        .ok_or(WorkspaceObjectError::BadWorkspaceObject)?
        .object();
    let service_monitor = layout_for(restored, ObjectId::new(0x7202))?;
    let python_console = layout_for(restored, ObjectId::new(0x7203))?;
    if service_monitor.x() != 2
        || service_monitor.y() != 0
        || service_monitor.width() != 2
        || service_monitor.height() != 2
        || service_monitor.z_order() != 1
        || python_console.x() != 0
        || python_console.y() != 2
        || python_console.width() != 4
        || python_console.height() != 2
        || python_console.z_order() != 2
    {
        return Err(WorkspaceObjectError::BadWorkspaceObject);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT");
    Ok(())
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset] = value as u8;
    bytes[offset + 1] = (value >> 8) as u8;
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    let mut remaining = value;
    let mut index = 0;
    while index < 8 {
        bytes[offset + index] = remaining as u8;
        remaining >>= 8;
        index += 1;
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from(bytes[offset]) | (u16::from(bytes[offset + 1]) << 8)
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    let mut value = 0;
    let mut index = 0;
    while index < 8 {
        value |= u64::from(bytes[offset + index]) << (index * 8);
        index += 1;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_session_record_captures_phase_5_window_ids() {
        let apps = register_first_party_apps().unwrap();

        let record = workspace_session_record(&apps).unwrap();

        assert_eq!(record.object_kind(), ObjectKind::WorkspaceSession);
        assert_eq!(record.field_count(), 4);
        assert_eq!(
            layout_for(record, ObjectId::new(0x7201))
                .unwrap()
                .object_id(),
            ObjectId::new(0x7201)
        );
        assert_eq!(
            layout_for(record, ObjectId::new(0x7204))
                .unwrap()
                .object_id(),
            ObjectId::new(0x7204)
        );
    }

    #[test]
    fn workspace_layout_round_trips_binding_geometry() {
        let apps = register_first_party_apps().unwrap();
        let record = workspace_session_record(&apps).unwrap();

        let console = layout_for(record, ObjectId::new(0x7203)).unwrap();

        assert_eq!(console.x(), 0);
        assert_eq!(console.y(), 2);
        assert_eq!(console.width(), 4);
        assert_eq!(console.height(), 2);
        assert_eq!(console.z_order(), 2);
    }

    #[test]
    fn workspace_session_survives_revision_history_current_record() {
        let apps = register_first_party_apps().unwrap();
        let record = workspace_session_record(&apps).unwrap();
        let mut identities = ServiceIdentityTable::new();
        let writer = identities.register_task(TaskId::new(90)).unwrap();
        let mut history = RevisionHistory::new();
        history.create_object(record, 200, writer).unwrap();

        let restored = history
            .current_revision(WORKSPACE_SESSION_OBJECT_ID)
            .unwrap()
            .object();

        assert_eq!(restored.object_kind(), ObjectKind::WorkspaceSession);
        assert_eq!(
            layout_for(restored, ObjectId::new(0x7202))
                .unwrap()
                .z_order(),
            1
        );
    }
}
