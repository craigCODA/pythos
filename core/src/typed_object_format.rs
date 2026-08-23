//! ADR 0022 fixed on-disk typed object record format.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::shell_objects::{ObjectId, ObjectKind};
use pythos_shared::package_abi::{
    OBJECT_KIND_PACKAGE, OBJECT_KIND_PACKAGE_DEFINED_OBJECT, OBJECT_KIND_SCHEMA_DEFINITION,
};
use pythos_shared::task_abi::{
    OBJECT_KIND_CAPABILITY_REQUEST, OBJECT_KIND_RELEVANCE_ASSERTION, OBJECT_KIND_TASK,
    OBJECT_KIND_TASK_EVENT, OBJECT_KIND_TASK_PROPOSAL, OBJECT_KIND_TASK_RELATION,
};

pub const FORMAT_VERSION: u16 = 1;
pub const MAX_FIELDS: usize = 4;
pub const FIELD_VALUE_CAPACITY: usize = 16;
pub const FIELD_SLOT_SIZE: usize = 24;
pub const HEADER_SIZE: usize = 24;
pub const RECORD_SIZE: usize = HEADER_SIZE + (MAX_FIELDS * FIELD_SLOT_SIZE);

const MAGIC: [u8; 4] = *b"PYOB";
const EMPTY_FIELD: TypedObjectField = TypedObjectField {
    field_id: 0,
    field_version: 0,
    value_len: 0,
    value: [0; FIELD_VALUE_CAPACITY],
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectFormatError {
    BadMagic,
    UnsupportedVersion,
    InvalidRecordLength,
    InvalidKind,
    FieldTableFull,
    FieldValueTooLarge,
    FieldReservedNonZero,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypedObjectField {
    field_id: u16,
    field_version: u16,
    value_len: u16,
    value: [u8; FIELD_VALUE_CAPACITY],
}

impl TypedObjectField {
    pub fn new(field_id: u16, field_version: u16, value: &[u8]) -> Result<Self, ObjectFormatError> {
        if value.len() > FIELD_VALUE_CAPACITY {
            return Err(ObjectFormatError::FieldValueTooLarge);
        }
        let mut field = Self {
            field_id,
            field_version,
            value_len: value.len() as u16,
            value: [0; FIELD_VALUE_CAPACITY],
        };
        field.value[..value.len()].copy_from_slice(value);
        Ok(field)
    }

    pub const fn field_id(self) -> u16 {
        self.field_id
    }

    pub const fn field_version(self) -> u16 {
        self.field_version
    }

    pub const fn value_len(self) -> u16 {
        self.value_len
    }

    pub fn value(self) -> [u8; FIELD_VALUE_CAPACITY] {
        self.value
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypedObjectRecord {
    object_id: ObjectId,
    object_kind: ObjectKind,
    schema_version: u16,
    field_count: usize,
    fields: [TypedObjectField; MAX_FIELDS],
}

impl TypedObjectRecord {
    pub const fn new(object_id: ObjectId, object_kind: ObjectKind, schema_version: u16) -> Self {
        Self {
            object_id,
            object_kind,
            schema_version,
            field_count: 0,
            fields: [EMPTY_FIELD; MAX_FIELDS],
        }
    }

    pub fn push_field(&mut self, field: TypedObjectField) -> Result<(), ObjectFormatError> {
        if self.field_count == MAX_FIELDS {
            return Err(ObjectFormatError::FieldTableFull);
        }
        self.fields[self.field_count] = field;
        self.field_count += 1;
        Ok(())
    }

    pub const fn object_id(self) -> ObjectId {
        self.object_id
    }

    pub const fn object_kind(self) -> ObjectKind {
        self.object_kind
    }

    pub const fn schema_version(self) -> u16 {
        self.schema_version
    }

    pub const fn field_count(self) -> usize {
        self.field_count
    }

    pub fn field(self, index: usize) -> Option<TypedObjectField> {
        if index >= self.field_count {
            None
        } else {
            Some(self.fields[index])
        }
    }

    pub fn encode(self) -> [u8; RECORD_SIZE] {
        let mut bytes = [0; RECORD_SIZE];
        bytes[0..4].copy_from_slice(&MAGIC);
        write_u16(&mut bytes, 4, FORMAT_VERSION);
        write_u16(&mut bytes, 6, RECORD_SIZE as u16);
        write_u64(&mut bytes, 8, self.object_id.raw());
        write_u16(&mut bytes, 16, kind_code(self.object_kind));
        write_u16(&mut bytes, 18, self.schema_version);
        write_u16(&mut bytes, 20, self.field_count as u16);
        write_u16(&mut bytes, 22, 0);
        let mut index = 0;
        while index < self.field_count {
            let field = self.fields[index];
            let offset = HEADER_SIZE + (index * FIELD_SLOT_SIZE);
            write_u16(&mut bytes, offset, field.field_id);
            write_u16(&mut bytes, offset + 2, field.field_version);
            write_u16(&mut bytes, offset + 4, field.value_len);
            write_u16(&mut bytes, offset + 6, 0);
            bytes[offset + 8..offset + 8 + FIELD_VALUE_CAPACITY].copy_from_slice(&field.value);
            index += 1;
        }
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ObjectFormatError> {
        if bytes.len() < RECORD_SIZE {
            return Err(ObjectFormatError::InvalidRecordLength);
        }
        if bytes[0] != MAGIC[0]
            || bytes[1] != MAGIC[1]
            || bytes[2] != MAGIC[2]
            || bytes[3] != MAGIC[3]
        {
            return Err(ObjectFormatError::BadMagic);
        }
        if read_u16(bytes, 4) != FORMAT_VERSION {
            return Err(ObjectFormatError::UnsupportedVersion);
        }
        if read_u16(bytes, 6) != RECORD_SIZE as u16 || read_u16(bytes, 22) != 0 {
            return Err(ObjectFormatError::InvalidRecordLength);
        }
        let field_count = read_u16(bytes, 20) as usize;
        if field_count > MAX_FIELDS {
            return Err(ObjectFormatError::InvalidRecordLength);
        }
        let mut record = Self::new(
            ObjectId::new(read_u64(bytes, 8)),
            kind_from_code(read_u16(bytes, 16))?,
            read_u16(bytes, 18),
        );
        let mut index = 0;
        while index < field_count {
            let offset = HEADER_SIZE + (index * FIELD_SLOT_SIZE);
            if read_u16(bytes, offset + 6) != 0 {
                return Err(ObjectFormatError::FieldReservedNonZero);
            }
            let value_len = read_u16(bytes, offset + 4);
            if usize::from(value_len) > FIELD_VALUE_CAPACITY {
                return Err(ObjectFormatError::FieldValueTooLarge);
            }
            let mut value = [0; FIELD_VALUE_CAPACITY];
            value.copy_from_slice(&bytes[offset + 8..offset + 8 + FIELD_VALUE_CAPACITY]);
            record.push_field(TypedObjectField {
                field_id: read_u16(bytes, offset),
                field_version: read_u16(bytes, offset + 2),
                value_len,
                value,
            })?;
            index += 1;
        }
        Ok(record)
    }
}

pub fn run_self_test() -> Result<(), ObjectFormatError> {
    let mut record =
        TypedObjectRecord::new(ObjectId::new(0x7202), ObjectKind::ServiceMonitorWindow, 1);
    record.push_field(TypedObjectField::new(1, 1, b"service")?)?;
    record.push_field(TypedObjectField::new(2, 2, b"ready")?)?;
    let decoded = TypedObjectRecord::decode(&record.encode())?;
    if decoded.object_id().raw() != 0x7202
        || decoded.object_kind() != ObjectKind::ServiceMonitorWindow
        || decoded.schema_version() != 1
    {
        return Err(ObjectFormatError::InvalidKind);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT:STABLE_ID");

    let first = decoded
        .field(0)
        .ok_or(ObjectFormatError::InvalidRecordLength)?;
    let second = decoded
        .field(1)
        .ok_or(ObjectFormatError::InvalidRecordLength)?;
    let first_value = first.value();
    let second_value = second.value();
    if decoded.field_count() != 2
        || first.field_id() != 1
        || first.field_version() != 1
        || first.value_len() != 7
        || &first_value[..7] != b"service"
        || second.field_id() != 2
        || second.field_version() != 2
        || second.value_len() != 5
        || &second_value[..5] != b"ready"
    {
        return Err(ObjectFormatError::InvalidRecordLength);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT:VERSIONED_FIELDS");
    Ok(())
}

fn kind_code(kind: ObjectKind) -> u16 {
    match kind {
        ObjectKind::ApplicationLauncherWindow => 1,
        ObjectKind::BootIdentitySurface => 2,
        ObjectKind::ServiceMonitorWindow => 3,
        ObjectKind::PythonConsoleWindow => 4,
        ObjectKind::SettingsPanelWindow => 5,
        ObjectKind::ButtonWidget => 6,
        ObjectKind::TextFieldWidget => 7,
        ObjectKind::WorkspaceSession => 8,
        ObjectKind::ObjectBrowserWindow => 9,
        ObjectKind::Note => 10,
        ObjectKind::NameBinding => 11,
        ObjectKind::Task => OBJECT_KIND_TASK,
        ObjectKind::TaskProposal => OBJECT_KIND_TASK_PROPOSAL,
        ObjectKind::TaskEvent => OBJECT_KIND_TASK_EVENT,
        ObjectKind::TaskRelation => OBJECT_KIND_TASK_RELATION,
        ObjectKind::RelevanceAssertion => OBJECT_KIND_RELEVANCE_ASSERTION,
        ObjectKind::CapabilityRequest => OBJECT_KIND_CAPABILITY_REQUEST,
        ObjectKind::Package => OBJECT_KIND_PACKAGE,
        ObjectKind::SchemaDefinition => OBJECT_KIND_SCHEMA_DEFINITION,
        ObjectKind::PackageDefinedObject => OBJECT_KIND_PACKAGE_DEFINED_OBJECT,
    }
}

fn kind_from_code(code: u16) -> Result<ObjectKind, ObjectFormatError> {
    match code {
        1 => Ok(ObjectKind::ApplicationLauncherWindow),
        2 => Ok(ObjectKind::BootIdentitySurface),
        3 => Ok(ObjectKind::ServiceMonitorWindow),
        4 => Ok(ObjectKind::PythonConsoleWindow),
        5 => Ok(ObjectKind::SettingsPanelWindow),
        6 => Ok(ObjectKind::ButtonWidget),
        7 => Ok(ObjectKind::TextFieldWidget),
        8 => Ok(ObjectKind::WorkspaceSession),
        9 => Ok(ObjectKind::ObjectBrowserWindow),
        10 => Ok(ObjectKind::Note),
        11 => Ok(ObjectKind::NameBinding),
        OBJECT_KIND_TASK => Ok(ObjectKind::Task),
        OBJECT_KIND_TASK_PROPOSAL => Ok(ObjectKind::TaskProposal),
        OBJECT_KIND_TASK_EVENT => Ok(ObjectKind::TaskEvent),
        OBJECT_KIND_TASK_RELATION => Ok(ObjectKind::TaskRelation),
        OBJECT_KIND_RELEVANCE_ASSERTION => Ok(ObjectKind::RelevanceAssertion),
        OBJECT_KIND_CAPABILITY_REQUEST => Ok(ObjectKind::CapabilityRequest),
        OBJECT_KIND_PACKAGE => Ok(ObjectKind::Package),
        OBJECT_KIND_SCHEMA_DEFINITION => Ok(ObjectKind::SchemaDefinition),
        OBJECT_KIND_PACKAGE_DEFINED_OBJECT => Ok(ObjectKind::PackageDefinedObject),
        _ => Err(ObjectFormatError::InvalidKind),
    }
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
    fn record_round_trips_stable_identity_and_kind() {
        let record =
            TypedObjectRecord::new(ObjectId::new(0xCAFE), ObjectKind::PythonConsoleWindow, 3);

        let decoded = TypedObjectRecord::decode(&record.encode()).unwrap();

        assert_eq!(decoded.object_id().raw(), 0xCAFE);
        assert_eq!(decoded.object_kind(), ObjectKind::PythonConsoleWindow);
        assert_eq!(decoded.schema_version(), 3);
    }

    #[test]
    fn versioned_fields_round_trip_without_presentation_binding() {
        let mut record = TypedObjectRecord::new(
            ObjectId::new(0x7201),
            ObjectKind::ApplicationLauncherWindow,
            1,
        );
        record
            .push_field(TypedObjectField::new(9, 2, b"launcher").unwrap())
            .unwrap();

        let decoded = TypedObjectRecord::decode(&record.encode()).unwrap();
        let field = decoded.field(0).unwrap();

        assert_eq!(field.field_id(), 9);
        assert_eq!(field.field_version(), 2);
        assert_eq!(field.value_len(), 8);
        assert_eq!(decoded.field(1), None);
    }

    #[test]
    fn note_kind_round_trips_with_stable_code() {
        let mut record = TypedObjectRecord::new(ObjectId::new(1042), ObjectKind::Note, 1);
        record
            .push_field(TypedObjectField::new(1, 1, b"hello").unwrap())
            .unwrap();

        let decoded = TypedObjectRecord::decode(&record.encode()).unwrap();
        let field = decoded.field(0).unwrap();

        assert_eq!(decoded.object_kind(), ObjectKind::Note);
        assert_eq!(decoded.object_id(), ObjectId::new(1042));
        assert_eq!(field.field_id(), 1);
        assert_eq!(field.value_len(), 5);
        assert_eq!(&field.value()[..5], b"hello");
    }

    #[test]
    fn name_binding_kind_round_trips_with_stable_code() {
        let mut record = TypedObjectRecord::new(ObjectId::new(0xA120), ObjectKind::NameBinding, 1);
        record
            .push_field(TypedObjectField::new(0x1201, 1, b"notes").unwrap())
            .unwrap();

        let encoded = record.encode();
        let decoded = TypedObjectRecord::decode(&encoded).unwrap();

        assert_eq!(read_u16(&encoded, 16), 11);
        assert_eq!(decoded.object_kind(), ObjectKind::NameBinding);
        assert_eq!(decoded.object_id(), ObjectId::new(0xA120));
    }

    #[test]
    fn task_object_kinds_round_trip_with_stable_codes() {
        let cases = [
            (ObjectKind::Task, OBJECT_KIND_TASK),
            (ObjectKind::TaskProposal, OBJECT_KIND_TASK_PROPOSAL),
            (ObjectKind::TaskEvent, OBJECT_KIND_TASK_EVENT),
            (ObjectKind::TaskRelation, OBJECT_KIND_TASK_RELATION),
            (
                ObjectKind::RelevanceAssertion,
                OBJECT_KIND_RELEVANCE_ASSERTION,
            ),
            (
                ObjectKind::CapabilityRequest,
                OBJECT_KIND_CAPABILITY_REQUEST,
            ),
        ];

        for (kind, code) in cases {
            let record = TypedObjectRecord::new(ObjectId::new(u64::from(code)), kind, 1);
            let mut bytes = record.encode();
            assert_eq!(read_u16(&bytes, 16), code);

            let decoded = TypedObjectRecord::decode(&bytes).unwrap();
            assert_eq!(decoded.object_kind(), kind);

            write_u16(&mut bytes, 16, code);
            assert_eq!(
                TypedObjectRecord::decode(&bytes).unwrap().object_kind(),
                kind
            );
        }
    }

    #[test]
    fn package_schema_object_creation_kinds_round_trip_with_stable_codes() {
        let cases = [
            (
                ObjectKind::Package,
                pythos_shared::package_abi::OBJECT_KIND_PACKAGE,
            ),
            (
                ObjectKind::SchemaDefinition,
                pythos_shared::package_abi::OBJECT_KIND_SCHEMA_DEFINITION,
            ),
            (
                ObjectKind::PackageDefinedObject,
                pythos_shared::package_abi::OBJECT_KIND_PACKAGE_DEFINED_OBJECT,
            ),
        ];

        for (kind, code) in cases {
            let record = TypedObjectRecord::new(ObjectId::new(u64::from(code)), kind, 1);
            let encoded = record.encode();
            let decoded = TypedObjectRecord::decode(&encoded).unwrap();

            assert_eq!(read_u16(&encoded, 16), code);
            assert_eq!(decoded.object_kind(), kind);
        }
    }

    #[test]
    fn package_defined_object_kind_wiring_preserves_serialized_layouts() {
        let record = TypedObjectRecord::new(
            ObjectId::new(u64::from(OBJECT_KIND_PACKAGE_DEFINED_OBJECT)),
            ObjectKind::PackageDefinedObject,
            1,
        );
        let encoded = record.encode();

        assert_eq!(OBJECT_KIND_PACKAGE, 30);
        assert_eq!(OBJECT_KIND_SCHEMA_DEFINITION, 31);
        assert_eq!(OBJECT_KIND_PACKAGE_DEFINED_OBJECT, 32);
        assert_eq!(RECORD_SIZE, 120);
        assert_eq!(encoded.len(), RECORD_SIZE);
        assert_eq!(read_u16(&encoded, 16), OBJECT_KIND_PACKAGE_DEFINED_OBJECT);
        assert_eq!(
            TypedObjectRecord::decode(&encoded).unwrap().object_kind(),
            ObjectKind::PackageDefinedObject
        );
    }

    #[test]
    fn package_schema_object_creation_unknown_code_still_denies() {
        let record = TypedObjectRecord::new(ObjectId::new(1), ObjectKind::ButtonWidget, 1);
        let mut bytes = record.encode();

        write_u16(&mut bytes, 16, 0x1300);

        assert_eq!(
            TypedObjectRecord::decode(&bytes),
            Err(ObjectFormatError::InvalidKind)
        );
    }

    #[test]
    fn rejects_bad_magic_and_unsupported_version() {
        let record = TypedObjectRecord::new(ObjectId::new(1), ObjectKind::ButtonWidget, 1);
        let mut bytes = record.encode();
        bytes[0] = b'X';
        assert_eq!(
            TypedObjectRecord::decode(&bytes),
            Err(ObjectFormatError::BadMagic)
        );

        let mut bytes = record.encode();
        write_u16(&mut bytes, 4, FORMAT_VERSION + 1);
        assert_eq!(
            TypedObjectRecord::decode(&bytes),
            Err(ObjectFormatError::UnsupportedVersion)
        );
    }

    #[test]
    fn rejects_invalid_kind_field_count_reserved_and_oversized_field() {
        let record = TypedObjectRecord::new(ObjectId::new(1), ObjectKind::ButtonWidget, 1);
        let mut bytes = record.encode();
        write_u16(&mut bytes, 16, 0xFFFF);
        assert_eq!(
            TypedObjectRecord::decode(&bytes),
            Err(ObjectFormatError::InvalidKind)
        );

        let mut bytes = record.encode();
        write_u16(&mut bytes, 20, (MAX_FIELDS + 1) as u16);
        assert_eq!(
            TypedObjectRecord::decode(&bytes),
            Err(ObjectFormatError::InvalidRecordLength)
        );

        let mut record = record;
        record
            .push_field(TypedObjectField::new(1, 1, b"x").unwrap())
            .unwrap();
        let mut bytes = record.encode();
        write_u16(&mut bytes, HEADER_SIZE + 6, 1);
        assert_eq!(
            TypedObjectRecord::decode(&bytes),
            Err(ObjectFormatError::FieldReservedNonZero)
        );

        assert_eq!(
            TypedObjectField::new(1, 1, b"0123456789abcdefg"),
            Err(ObjectFormatError::FieldValueTooLarge)
        );
    }
}
