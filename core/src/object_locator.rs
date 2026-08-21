//! Phase 12 capability-scoped object locator resolver.

#![cfg_attr(test, allow(dead_code))]
#![cfg_attr(feature = "verify", allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::{
    capabilities::{CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask},
    object_relationships::{BoundedRelationshipStore, ObjectRelationship, RelationshipKind},
    revision_history::BoundedRevisionHistory,
    service_identity::ServiceId,
    shell_objects::{ObjectId, ObjectKind},
    typed_object_format::{TypedObjectField, TypedObjectRecord},
};

pub const LOCATOR_ABI_MAJOR: u16 = 0;
pub const LOCATOR_ABI_MINOR: u16 = 1;
pub const MAX_LOCATOR_SEGMENTS: usize = 4;
pub const MAX_LOCATOR_SEGMENT_BYTES: usize = crate::typed_object_format::FIELD_VALUE_CAPACITY;
pub const LOCATOR_FIELD_SEGMENT: u16 = 0x1201;
const LOCATOR_TRAVERSAL_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ);
const EMPTY_SEGMENT: LocatorSegment = LocatorSegment {
    len: 0,
    bytes: [0; MAX_LOCATOR_SEGMENT_BYTES],
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocatorSyntaxError {
    Empty,
    HostAbsolute,
    EmptySegment,
    NavigationSegment,
    DrivePrefix,
    UriScheme,
    Wildcard,
    ShellExpansion,
    SegmentTooLong,
    TooManySegments,
    InvalidCharacter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectLocatorError {
    Syntax(LocatorSyntaxError),
    MissingTraversalAuthority,
    TraversalAuthorityDenied(CapabilityError),
    MissingSegment,
    NameCollision,
    MalformedBinding,
    StaleBinding,
    FinalObjectAuthorityDenied(CapabilityError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LocatorSegment {
    len: u8,
    bytes: [u8; MAX_LOCATOR_SEGMENT_BYTES],
}

impl LocatorSegment {
    fn parse(bytes: &[u8]) -> Result<Self, LocatorSyntaxError> {
        if bytes.is_empty() {
            return Err(LocatorSyntaxError::EmptySegment);
        }
        if bytes == b"." || bytes == b".." {
            return Err(LocatorSyntaxError::NavigationSegment);
        }
        if bytes.len() > MAX_LOCATOR_SEGMENT_BYTES {
            return Err(LocatorSyntaxError::SegmentTooLong);
        }

        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'*' | b'?' | b'[' | b']' => return Err(LocatorSyntaxError::Wildcard),
                b'~' | b'$' | b'{' | b'}' => return Err(LocatorSyntaxError::ShellExpansion),
                b':' | b'\\' => return Err(LocatorSyntaxError::InvalidCharacter),
                byte if is_locator_byte(byte) => {}
                _ => return Err(LocatorSyntaxError::InvalidCharacter),
            }
            index += 1;
        }

        let mut segment = EMPTY_SEGMENT;
        segment.len = bytes.len() as u8;
        segment.bytes[..bytes.len()].copy_from_slice(bytes);
        Ok(segment)
    }

    fn matches_field(self, field: TypedObjectField) -> bool {
        let value = field.value();
        let len = field.value_len() as usize;
        usize::from(self.len) == len && self.bytes[..len] == value[..len]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ParsedLocator {
    segments: [LocatorSegment; MAX_LOCATOR_SEGMENTS],
    count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectLocatorRequest {
    caller: ServiceId,
    root_namespace: ObjectId,
    traversal_authority: [Option<CapabilityHandle>; MAX_LOCATOR_SEGMENTS],
    final_authority: CapabilityHandle,
    final_rights: RightsMask,
}

impl ObjectLocatorRequest {
    pub fn new(
        caller: ServiceId,
        root_namespace: ObjectId,
        root_authority: CapabilityHandle,
        final_authority: CapabilityHandle,
        final_rights: RightsMask,
    ) -> Self {
        let mut traversal_authority = [None; MAX_LOCATOR_SEGMENTS];
        traversal_authority[0] = Some(root_authority);
        Self {
            caller,
            root_namespace,
            traversal_authority,
            final_authority,
            final_rights,
        }
    }

    pub fn set_traversal_authority(
        &mut self,
        segment_index: usize,
        authority: CapabilityHandle,
    ) -> bool {
        if segment_index >= MAX_LOCATOR_SEGMENTS {
            return false;
        }
        self.traversal_authority[segment_index] = Some(authority);
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectLocatorResult {
    object_id: ObjectId,
    object_kind: ObjectKind,
    revision: u64,
    relationship_path: [Option<ObjectRelationship>; MAX_LOCATOR_SEGMENTS * 2],
    relationship_count: usize,
}

impl ObjectLocatorResult {
    pub const fn object_id(self) -> ObjectId {
        self.object_id
    }

    pub const fn object_kind(self) -> ObjectKind {
        self.object_kind
    }

    pub const fn revision(self) -> u64 {
        self.revision
    }

    pub const fn relationship_count(self) -> usize {
        self.relationship_count
    }

    pub const fn relationship(self, index: usize) -> Option<ObjectRelationship> {
        if index < self.relationship_count {
            self.relationship_path[index]
        } else {
            None
        }
    }
}

pub fn validate_locator(locator: &str) -> Result<(), LocatorSyntaxError> {
    parse_locator(locator).map(|_| ())
}

pub fn resolve_locator<
    const OBJECT_CAPACITY: usize,
    const RELATIONSHIP_CAPACITY: usize,
    const CURRENT_CAPACITY: usize,
    const REVISION_CAPACITY: usize,
>(
    capabilities: &CapabilityTable,
    relationships: &BoundedRelationshipStore<OBJECT_CAPACITY, RELATIONSHIP_CAPACITY>,
    revisions: &BoundedRevisionHistory<CURRENT_CAPACITY, REVISION_CAPACITY>,
    request: ObjectLocatorRequest,
    locator: &str,
) -> Result<ObjectLocatorResult, ObjectLocatorError> {
    let parsed = parse_locator(locator).map_err(ObjectLocatorError::Syntax)?;
    let mut current_namespace = request.root_namespace;
    let mut relationship_path = [None; MAX_LOCATOR_SEGMENTS * 2];
    let mut relationship_count = 0usize;

    let mut segment_index = 0;
    while segment_index < parsed.count {
        let traversal_authority = request.traversal_authority[segment_index]
            .ok_or(ObjectLocatorError::MissingTraversalAuthority)?;
        capabilities
            .validate(
                request.caller,
                traversal_authority,
                ResourceId::new(current_namespace.raw()),
                LOCATOR_TRAVERSAL_RIGHTS,
            )
            .map_err(ObjectLocatorError::TraversalAuthorityDenied)?;

        let step = find_binding(
            relationships,
            revisions,
            current_namespace,
            parsed.segments[segment_index],
        )?;
        relationship_path[relationship_count] = Some(step.namespace_to_binding);
        relationship_count += 1;
        relationship_path[relationship_count] = Some(step.binding_to_target);
        relationship_count += 1;
        current_namespace = step.target;
        segment_index += 1;
    }

    let resolved = revisions
        .current_revision(current_namespace)
        .ok_or(ObjectLocatorError::StaleBinding)?;
    capabilities
        .validate(
            request.caller,
            request.final_authority,
            ResourceId::new(current_namespace.raw()),
            request.final_rights,
        )
        .map_err(ObjectLocatorError::FinalObjectAuthorityDenied)?;

    Ok(ObjectLocatorResult {
        object_id: current_namespace,
        object_kind: resolved.object().object_kind(),
        revision: resolved.revision(),
        relationship_path,
        relationship_count,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BindingStep {
    namespace_to_binding: ObjectRelationship,
    binding_to_target: ObjectRelationship,
    target: ObjectId,
}

fn parse_locator(locator: &str) -> Result<ParsedLocator, LocatorSyntaxError> {
    let bytes = locator.as_bytes();
    if bytes.is_empty() {
        return Err(LocatorSyntaxError::Empty);
    }
    if bytes[0] == b'/' || bytes[0] == b'\\' {
        return Err(LocatorSyntaxError::HostAbsolute);
    }
    if bytes.len() >= 2 && is_ascii_alpha(bytes[0]) && bytes[1] == b':' {
        return Err(LocatorSyntaxError::DrivePrefix);
    }
    if contains_uri_scheme(bytes) {
        return Err(LocatorSyntaxError::UriScheme);
    }

    let mut parsed = ParsedLocator {
        segments: [EMPTY_SEGMENT; MAX_LOCATOR_SEGMENTS],
        count: 0,
    };
    let mut start = 0usize;
    let mut index = 0usize;
    while index <= bytes.len() {
        if index == bytes.len() || bytes[index] == b'/' {
            if start == index {
                return Err(LocatorSyntaxError::EmptySegment);
            }
            if parsed.count == MAX_LOCATOR_SEGMENTS {
                return Err(LocatorSyntaxError::TooManySegments);
            }
            parsed.segments[parsed.count] = LocatorSegment::parse(&bytes[start..index])?;
            parsed.count += 1;
            start = index + 1;
        } else if bytes[index] == b'\\' {
            return Err(LocatorSyntaxError::HostAbsolute);
        }
        index += 1;
    }
    Ok(parsed)
}

fn find_binding<
    const OBJECT_CAPACITY: usize,
    const RELATIONSHIP_CAPACITY: usize,
    const CURRENT_CAPACITY: usize,
    const REVISION_CAPACITY: usize,
>(
    relationships: &BoundedRelationshipStore<OBJECT_CAPACITY, RELATIONSHIP_CAPACITY>,
    revisions: &BoundedRevisionHistory<CURRENT_CAPACITY, REVISION_CAPACITY>,
    namespace: ObjectId,
    segment: LocatorSegment,
) -> Result<BindingStep, ObjectLocatorError> {
    let mut found = None;
    let records = relationships.relationship_records();
    let mut index = 0usize;
    while index < RELATIONSHIP_CAPACITY {
        if let Some(namespace_to_binding) = records[index]
            && namespace_to_binding.source() == namespace
            && namespace_to_binding.kind() == RelationshipKind::NameBinding
        {
            let binding_record = revisions
                .current_revision(namespace_to_binding.target())
                .ok_or(ObjectLocatorError::StaleBinding)?
                .object();
            if binding_record.object_kind() != ObjectKind::NameBinding {
                return Err(ObjectLocatorError::MalformedBinding);
            }
            if binding_record_matches_segment(binding_record, segment)? {
                let binding_to_target =
                    find_binding_target(relationships, namespace_to_binding.target())?;
                if found.is_some() {
                    return Err(ObjectLocatorError::NameCollision);
                }
                found = Some(BindingStep {
                    namespace_to_binding,
                    binding_to_target,
                    target: binding_to_target.target(),
                });
            }
        }
        index += 1;
    }
    found.ok_or(ObjectLocatorError::MissingSegment)
}

fn find_binding_target<const OBJECT_CAPACITY: usize, const RELATIONSHIP_CAPACITY: usize>(
    relationships: &BoundedRelationshipStore<OBJECT_CAPACITY, RELATIONSHIP_CAPACITY>,
    binding_object: ObjectId,
) -> Result<ObjectRelationship, ObjectLocatorError> {
    let records = relationships.relationship_records();
    let mut found = None;
    let mut index = 0usize;
    while index < RELATIONSHIP_CAPACITY {
        if let Some(relationship) = records[index]
            && relationship.source() == binding_object
            && relationship.kind() == RelationshipKind::BindingTarget
        {
            if found.is_some() {
                return Err(ObjectLocatorError::MalformedBinding);
            }
            found = Some(relationship);
        }
        index += 1;
    }
    found.ok_or(ObjectLocatorError::MalformedBinding)
}

fn binding_record_matches_segment(
    record: TypedObjectRecord,
    segment: LocatorSegment,
) -> Result<bool, ObjectLocatorError> {
    let mut index = 0usize;
    while index < record.field_count() {
        if let Some(field) = record.field(index)
            && field.field_id() == LOCATOR_FIELD_SEGMENT
        {
            LocatorSegment::parse(&field.value()[..field.value_len() as usize])
                .map_err(|_| ObjectLocatorError::MalformedBinding)?;
            return Ok(segment.matches_field(field));
        }
        index += 1;
    }
    Err(ObjectLocatorError::MalformedBinding)
}

pub fn run_self_test() -> Result<(), ObjectLocatorError> {
    use crate::{
        capabilities::ResourceId,
        dynamic_object_store::DynamicObjectStore,
        object_relationships::{BoundedRelationshipStore, SHELL_WORKSPACE_OBJECT_ID},
        revision_history::BoundedRevisionHistory,
        service_identity::ServiceIdentityTable,
        tasks::TaskId,
    };

    type SelfTestRelationships = BoundedRelationshipStore<4, 4>;
    type SelfTestRevisions = BoundedRevisionHistory<4, 4>;

    const BINDING_OBJECT_ID: ObjectId = ObjectId::new(0xA120);
    const TARGET_OBJECT_ID: ObjectId = ObjectId::new(0xA121);

    let mut identities = ServiceIdentityTable::new();
    let caller = identities
        .register_task(TaskId::new(212))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let root = ObjectId::new(SHELL_WORKSPACE_OBJECT_ID);
    let root_object = TypedObjectRecord::new(root, ObjectKind::WorkspaceSession, 1);
    let mut binding = TypedObjectRecord::new(BINDING_OBJECT_ID, ObjectKind::NameBinding, 1);
    binding
        .push_field(
            TypedObjectField::new(LOCATOR_FIELD_SEGMENT, 1, b"notes")
                .map_err(|_| ObjectLocatorError::MalformedBinding)?,
        )
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let target = TypedObjectRecord::new(TARGET_OBJECT_ID, ObjectKind::Note, 1);

    let mut stored_objects =
        DynamicObjectStore::new(160, 4).map_err(|_| ObjectLocatorError::MalformedBinding)?;
    stored_objects
        .create_object(binding)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    stored_objects
        .create_object(target)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    if stored_objects.object(TARGET_OBJECT_ID) != Some(target) {
        return Err(ObjectLocatorError::StaleBinding);
    }

    let mut relationships = SelfTestRelationships::new();
    relationships
        .insert_object(root_object)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    relationships
        .insert_object(binding)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    relationships
        .insert_object(target)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    relationships
        .add_relationship(ObjectRelationship::new(
            root,
            RelationshipKind::NameBinding,
            BINDING_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    relationships
        .add_relationship(ObjectRelationship::new(
            BINDING_OBJECT_ID,
            RelationshipKind::BindingTarget,
            TARGET_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;

    let mut revisions = SelfTestRevisions::new();
    revisions
        .create_object(root_object, 1, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    revisions
        .create_object(binding, 2, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    revisions
        .create_object(target, 3, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;

    let mut capabilities = CapabilityTable::new();
    let root_capability = capabilities
        .grant(
            caller,
            ResourceId::new(root.raw()),
            RightsMask::new(RightsMask::READ),
        )
        .map_err(|error| ObjectLocatorError::TraversalAuthorityDenied(error))?;
    let target_capability = capabilities
        .grant(
            caller,
            ResourceId::new(TARGET_OBJECT_ID.raw()),
            RightsMask::new(RightsMask::READ),
        )
        .map_err(|error| ObjectLocatorError::FinalObjectAuthorityDenied(error))?;

    let request = ObjectLocatorRequest::new(
        caller,
        root,
        root_capability,
        target_capability,
        RightsMask::new(RightsMask::READ),
    );
    let resolved = resolve_locator(&capabilities, &relationships, &revisions, request, "notes")?;
    if resolved.object_id() != TARGET_OBJECT_ID
        || resolved.revision() != 1
        || resolved.relationship_count() != 2
    {
        return Err(ObjectLocatorError::MissingSegment);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:LOCATOR:RESOLVED");

    if validate_locator("notes/../secret") != Err(LocatorSyntaxError::NavigationSegment) {
        return Err(ObjectLocatorError::Syntax(
            LocatorSyntaxError::NavigationSegment,
        ));
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:LOCATOR:INVALID_NAVIGATION_DENIED");

    let traversal_denied_request = ObjectLocatorRequest::new(
        caller,
        root,
        target_capability,
        target_capability,
        RightsMask::new(RightsMask::READ),
    );
    if resolve_locator(
        &capabilities,
        &relationships,
        &revisions,
        traversal_denied_request,
        "notes",
    ) != Err(ObjectLocatorError::TraversalAuthorityDenied(
        CapabilityError::WrongResource,
    )) {
        return Err(ObjectLocatorError::MissingTraversalAuthority);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:LOCATOR:TRAVERSAL_AUTH_DENIED");

    let final_denied_request = ObjectLocatorRequest::new(
        caller,
        root,
        root_capability,
        root_capability,
        RightsMask::new(RightsMask::READ),
    );
    if resolve_locator(
        &capabilities,
        &relationships,
        &revisions,
        final_denied_request,
        "notes",
    ) != Err(ObjectLocatorError::FinalObjectAuthorityDenied(
        CapabilityError::WrongResource,
    )) {
        return Err(ObjectLocatorError::FinalObjectAuthorityDenied(
            CapabilityError::WrongResource,
        ));
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:LOCATOR:FINAL_AUTH_DENIED");
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:OBJECT_LOCATOR_RESOLUTION_READY");
    Ok(())
}

pub fn run_adversarial_self_test() -> Result<(), ObjectLocatorError> {
    use crate::{
        object_relationships::{BoundedRelationshipStore, SHELL_WORKSPACE_OBJECT_ID},
        revision_history::BoundedRevisionHistory,
        service_identity::ServiceIdentityTable,
        tasks::TaskId,
    };

    type SmallRelationships = BoundedRelationshipStore<4, 4>;
    type SmallRevisions = BoundedRevisionHistory<4, 4>;
    type MultiSegmentRelationships = BoundedRelationshipStore<5, 4>;
    type MultiSegmentRevisions = BoundedRevisionHistory<5, 5>;
    type AdversarialRelationships = BoundedRelationshipStore<8, 8>;
    type AdversarialRevisions = BoundedRevisionHistory<8, 8>;

    const ROOT_TASK_ID: TaskId = TaskId::new(216);
    const ROOT_OBJECT_ID: ObjectId = ObjectId::new(SHELL_WORKSPACE_OBJECT_ID);
    const BINDING_OBJECT_ID: ObjectId = ObjectId::new(0xA220);
    const TARGET_OBJECT_ID: ObjectId = ObjectId::new(0xA221);
    const CHILD_NAMESPACE_OBJECT_ID: ObjectId = ObjectId::new(0xA222);
    const CHILD_BINDING_OBJECT_ID: ObjectId = ObjectId::new(0xA223);
    const CHILD_TARGET_OBJECT_ID: ObjectId = ObjectId::new(0xA224);
    const SECOND_BINDING_OBJECT_ID: ObjectId = ObjectId::new(0xA225);
    const SECOND_TARGET_OBJECT_ID: ObjectId = ObjectId::new(0xA226);
    const ALTERNATE_ROOT_OBJECT_ID: ObjectId = ObjectId::new(0xA227);

    if validate_locator("notes//secret") != Err(LocatorSyntaxError::EmptySegment) {
        return Err(ObjectLocatorError::Syntax(LocatorSyntaxError::EmptySegment));
    }
    locator_marker("PYTHOS:CORE:LOCATOR:EMPTY_SEGMENT_DENIED");

    let mut identities = ServiceIdentityTable::new();
    let caller = identities
        .register_task(ROOT_TASK_ID)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let root = ROOT_OBJECT_ID;
    let root_object = TypedObjectRecord::new(root, ObjectKind::WorkspaceSession, 1);
    let binding = locator_binding_record(BINDING_OBJECT_ID, b"notes")?;
    let target = TypedObjectRecord::new(TARGET_OBJECT_ID, ObjectKind::Note, 1);

    let mut stale_relationships = SmallRelationships::new();
    stale_relationships
        .insert_object(root_object)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    stale_relationships
        .insert_object(binding)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    stale_relationships
        .insert_object(target)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    stale_relationships
        .add_relationship(ObjectRelationship::new(
            root,
            RelationshipKind::NameBinding,
            BINDING_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    stale_relationships
        .add_relationship(ObjectRelationship::new(
            BINDING_OBJECT_ID,
            RelationshipKind::BindingTarget,
            TARGET_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let mut stale_revisions = SmallRevisions::new();
    stale_revisions
        .create_object(root_object, 1, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    stale_revisions
        .create_object(target, 2, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let mut capabilities = CapabilityTable::new();
    let root_capability = capabilities
        .grant(
            caller,
            ResourceId::new(root.raw()),
            LOCATOR_TRAVERSAL_RIGHTS,
        )
        .map_err(ObjectLocatorError::TraversalAuthorityDenied)?;
    let target_capability = capabilities
        .grant(
            caller,
            ResourceId::new(TARGET_OBJECT_ID.raw()),
            RightsMask::new(RightsMask::READ),
        )
        .map_err(ObjectLocatorError::FinalObjectAuthorityDenied)?;
    expect_locator_error(
        resolve_locator(
            &capabilities,
            &stale_relationships,
            &stale_revisions,
            ObjectLocatorRequest::new(
                caller,
                root,
                root_capability,
                target_capability,
                RightsMask::new(RightsMask::READ),
            ),
            "notes",
        ),
        ObjectLocatorError::StaleBinding,
    )?;
    locator_marker("PYTHOS:CORE:LOCATOR:STALE_BINDING_DENIED");

    let mut missing_relationships = SmallRelationships::new();
    missing_relationships
        .insert_object(root_object)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let mut missing_revisions = SmallRevisions::new();
    missing_revisions
        .create_object(root_object, 1, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    expect_locator_error(
        resolve_locator(
            &capabilities,
            &missing_relationships,
            &missing_revisions,
            ObjectLocatorRequest::new(
                caller,
                root,
                root_capability,
                target_capability,
                RightsMask::new(RightsMask::READ),
            ),
            "missing",
        ),
        ObjectLocatorError::MissingSegment,
    )?;
    locator_marker("PYTHOS:CORE:LOCATOR:MISSING_SEGMENT_DENIED");

    let first_binding = binding;
    let child_namespace =
        TypedObjectRecord::new(CHILD_NAMESPACE_OBJECT_ID, ObjectKind::WorkspaceSession, 1);
    let second_binding = locator_binding_record(CHILD_BINDING_OBJECT_ID, b"today")?;
    let child_target = TypedObjectRecord::new(CHILD_TARGET_OBJECT_ID, ObjectKind::Note, 1);
    let mut multi_relationships = MultiSegmentRelationships::new();
    multi_relationships
        .insert_object(root_object)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_relationships
        .insert_object(first_binding)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_relationships
        .insert_object(child_namespace)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_relationships
        .insert_object(second_binding)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_relationships
        .insert_object(child_target)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_relationships
        .add_relationship(ObjectRelationship::new(
            root,
            RelationshipKind::NameBinding,
            BINDING_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_relationships
        .add_relationship(ObjectRelationship::new(
            BINDING_OBJECT_ID,
            RelationshipKind::BindingTarget,
            CHILD_NAMESPACE_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_relationships
        .add_relationship(ObjectRelationship::new(
            CHILD_NAMESPACE_OBJECT_ID,
            RelationshipKind::NameBinding,
            CHILD_BINDING_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_relationships
        .add_relationship(ObjectRelationship::new(
            CHILD_BINDING_OBJECT_ID,
            RelationshipKind::BindingTarget,
            CHILD_TARGET_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let mut multi_revisions = MultiSegmentRevisions::new();
    multi_revisions
        .create_object(root_object, 1, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_revisions
        .create_object(first_binding, 2, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_revisions
        .create_object(child_namespace, 3, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_revisions
        .create_object(second_binding, 4, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    multi_revisions
        .create_object(child_target, 5, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let child_target_capability = capabilities
        .grant(
            caller,
            ResourceId::new(CHILD_TARGET_OBJECT_ID.raw()),
            RightsMask::new(RightsMask::READ),
        )
        .map_err(ObjectLocatorError::FinalObjectAuthorityDenied)?;
    expect_locator_error(
        resolve_locator(
            &capabilities,
            &multi_relationships,
            &multi_revisions,
            ObjectLocatorRequest::new(
                caller,
                root,
                root_capability,
                child_target_capability,
                RightsMask::new(RightsMask::READ),
            ),
            "notes/today",
        ),
        ObjectLocatorError::MissingTraversalAuthority,
    )?;
    locator_marker("PYTHOS:CORE:LOCATOR:MISSING_TRAVERSAL_DENIED");

    expect_locator_error(
        resolve_locator(
            &capabilities,
            &stale_relationships,
            &stale_revisions,
            ObjectLocatorRequest::new(
                caller,
                root,
                root_capability,
                CapabilityHandle::from_parts(99, 1),
                RightsMask::new(RightsMask::READ),
            ),
            "notes",
        ),
        ObjectLocatorError::StaleBinding,
    )?;
    let mut final_revisions = SmallRevisions::new();
    final_revisions
        .create_object(root_object, 1, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    final_revisions
        .create_object(binding, 2, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    final_revisions
        .create_object(target, 3, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    expect_locator_error(
        resolve_locator(
            &capabilities,
            &stale_relationships,
            &final_revisions,
            ObjectLocatorRequest::new(
                caller,
                root,
                root_capability,
                CapabilityHandle::from_parts(99, 1),
                RightsMask::new(RightsMask::READ),
            ),
            "notes",
        ),
        ObjectLocatorError::FinalObjectAuthorityDenied(CapabilityError::InvalidHandle),
    )?;
    locator_marker("PYTHOS:CORE:LOCATOR:MISSING_FINAL_AUTH_DENIED");

    let second_same_name_binding = locator_binding_record(SECOND_BINDING_OBJECT_ID, b"notes")?;
    let second_target = TypedObjectRecord::new(SECOND_TARGET_OBJECT_ID, ObjectKind::Note, 1);
    let mut collision_relationships = AdversarialRelationships::new();
    collision_relationships
        .insert_object(root_object)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    collision_relationships
        .insert_object(binding)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    collision_relationships
        .insert_object(second_same_name_binding)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    collision_relationships
        .insert_object(target)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    collision_relationships
        .insert_object(second_target)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    collision_relationships
        .add_relationship(ObjectRelationship::new(
            root,
            RelationshipKind::NameBinding,
            BINDING_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    collision_relationships
        .add_relationship(ObjectRelationship::new(
            BINDING_OBJECT_ID,
            RelationshipKind::BindingTarget,
            TARGET_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    collision_relationships
        .add_relationship(ObjectRelationship::new(
            root,
            RelationshipKind::NameBinding,
            SECOND_BINDING_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    collision_relationships
        .add_relationship(ObjectRelationship::new(
            SECOND_BINDING_OBJECT_ID,
            RelationshipKind::BindingTarget,
            SECOND_TARGET_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let mut collision_revisions = AdversarialRevisions::new();
    collision_revisions
        .create_object(root_object, 1, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    collision_revisions
        .create_object(binding, 2, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    collision_revisions
        .create_object(second_same_name_binding, 3, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    expect_locator_error(
        resolve_locator(
            &capabilities,
            &collision_relationships,
            &collision_revisions,
            ObjectLocatorRequest::new(
                caller,
                root,
                root_capability,
                target_capability,
                RightsMask::new(RightsMask::READ),
            ),
            "notes",
        ),
        ObjectLocatorError::NameCollision,
    )?;
    locator_marker("PYTHOS:CORE:LOCATOR:NAME_COLLISION_DENIED");

    let mut link_confusion_relationships = SmallRelationships::new();
    link_confusion_relationships
        .insert_object(root_object)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    link_confusion_relationships
        .insert_object(target)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    link_confusion_relationships
        .add_relationship(ObjectRelationship::new(
            root,
            RelationshipKind::BelongsTo,
            TARGET_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    expect_locator_error(
        resolve_locator(
            &capabilities,
            &link_confusion_relationships,
            &final_revisions,
            ObjectLocatorRequest::new(
                caller,
                root,
                root_capability,
                target_capability,
                RightsMask::new(RightsMask::READ),
            ),
            "notes",
        ),
        ObjectLocatorError::MissingSegment,
    )?;
    locator_marker("PYTHOS:CORE:LOCATOR:LINK_CONFUSION_DENIED");

    let alternate_root =
        TypedObjectRecord::new(ALTERNATE_ROOT_OBJECT_ID, ObjectKind::WorkspaceSession, 1);
    let mut global_relationships = AdversarialRelationships::new();
    global_relationships
        .insert_object(root_object)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    global_relationships
        .insert_object(alternate_root)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    global_relationships
        .insert_object(binding)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    global_relationships
        .insert_object(target)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    global_relationships
        .add_relationship(ObjectRelationship::new(
            root,
            RelationshipKind::NameBinding,
            BINDING_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    global_relationships
        .add_relationship(ObjectRelationship::new(
            BINDING_OBJECT_ID,
            RelationshipKind::BindingTarget,
            TARGET_OBJECT_ID,
        ))
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let mut global_revisions = AdversarialRevisions::new();
    global_revisions
        .create_object(root_object, 1, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    global_revisions
        .create_object(alternate_root, 2, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    global_revisions
        .create_object(binding, 3, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    global_revisions
        .create_object(target, 4, caller)
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    let alternate_root_capability = capabilities
        .grant(
            caller,
            ResourceId::new(ALTERNATE_ROOT_OBJECT_ID.raw()),
            LOCATOR_TRAVERSAL_RIGHTS,
        )
        .map_err(ObjectLocatorError::TraversalAuthorityDenied)?;
    expect_locator_error(
        resolve_locator(
            &capabilities,
            &global_relationships,
            &global_revisions,
            ObjectLocatorRequest::new(
                caller,
                ALTERNATE_ROOT_OBJECT_ID,
                alternate_root_capability,
                target_capability,
                RightsMask::new(RightsMask::READ),
            ),
            "notes",
        ),
        ObjectLocatorError::MissingSegment,
    )?;
    locator_marker("PYTHOS:CORE:LOCATOR:GLOBAL_ROOT_DENIED");
    locator_marker("PYTHOS:CORE:PATH_ADVERSARIAL_SUITE_READY");
    locator_marker("PYTHOS:CORE:PHASE_12_COMPLETE");
    Ok(())
}

fn locator_binding_record(
    object_id: ObjectId,
    segment: &[u8],
) -> Result<TypedObjectRecord, ObjectLocatorError> {
    let mut binding = TypedObjectRecord::new(object_id, ObjectKind::NameBinding, 1);
    binding
        .push_field(
            TypedObjectField::new(LOCATOR_FIELD_SEGMENT, 1, segment)
                .map_err(|_| ObjectLocatorError::MalformedBinding)?,
        )
        .map_err(|_| ObjectLocatorError::MalformedBinding)?;
    Ok(binding)
}

fn expect_locator_error(
    result: Result<ObjectLocatorResult, ObjectLocatorError>,
    expected: ObjectLocatorError,
) -> Result<(), ObjectLocatorError> {
    if result == Err(expected) {
        Ok(())
    } else {
        Err(expected)
    }
}

fn locator_marker(marker: &str) {
    #[cfg(not(test))]
    serial::write_line(marker);
    #[cfg(test)]
    let _ = marker;
}

fn contains_uri_scheme(bytes: &[u8]) -> bool {
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'/' {
            return false;
        }
        if bytes[index] == b':' {
            return index > 0 && is_ascii_alpha(bytes[0]);
        }
        index += 1;
    }
    false
}

fn is_locator_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

fn is_ascii_alpha(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        capabilities::{ResourceId, RightsMask},
        object_relationships::{
            BoundedRelationshipStore, ObjectRelationship, RelationshipKind,
            SHELL_WORKSPACE_OBJECT_ID,
        },
        revision_history::BoundedRevisionHistory,
        service_identity::ServiceIdentityTable,
        tasks::TaskId,
        typed_object_format::{TypedObjectField, TypedObjectRecord},
    };

    type TestRelationships = BoundedRelationshipStore<4, 4>;
    type TestRevisions = BoundedRevisionHistory<4, 4>;
    type MultiSegmentRelationships = BoundedRelationshipStore<5, 4>;
    type MultiSegmentRevisions = BoundedRevisionHistory<5, 5>;
    type AdversarialRelationships = BoundedRelationshipStore<8, 8>;
    type AdversarialRevisions = BoundedRevisionHistory<8, 8>;

    const BINDING_OBJECT_ID: ObjectId = ObjectId::new(0xA120);
    const TARGET_OBJECT_ID: ObjectId = ObjectId::new(0xA121);
    const CHILD_NAMESPACE_OBJECT_ID: ObjectId = ObjectId::new(0xA122);
    const CHILD_BINDING_OBJECT_ID: ObjectId = ObjectId::new(0xA123);
    const CHILD_TARGET_OBJECT_ID: ObjectId = ObjectId::new(0xA124);
    const SECOND_BINDING_OBJECT_ID: ObjectId = ObjectId::new(0xA125);
    const SECOND_TARGET_OBJECT_ID: ObjectId = ObjectId::new(0xA126);
    const ALTERNATE_ROOT_OBJECT_ID: ObjectId = ObjectId::new(0xA127);

    fn name_binding_record(object_id: ObjectId, segment: &[u8]) -> TypedObjectRecord {
        let mut binding = TypedObjectRecord::new(object_id, ObjectKind::NameBinding, 1);
        binding
            .push_field(TypedObjectField::new(LOCATOR_FIELD_SEGMENT, 1, segment).unwrap())
            .unwrap();
        binding
    }

    struct LocatorFixture {
        capabilities: CapabilityTable,
        relationships: TestRelationships,
        revisions: TestRevisions,
        caller: ServiceId,
        root: ObjectId,
        root_capability: CapabilityHandle,
        target_capability: CapabilityHandle,
    }

    impl LocatorFixture {
        fn new() -> Self {
            let mut identities = ServiceIdentityTable::new();
            let caller = identities.register_task(TaskId::new(212)).unwrap();
            let root = ObjectId::new(SHELL_WORKSPACE_OBJECT_ID);
            let root_object = TypedObjectRecord::new(root, ObjectKind::WorkspaceSession, 1);
            let binding = name_binding_record(BINDING_OBJECT_ID, b"notes");
            let target = TypedObjectRecord::new(TARGET_OBJECT_ID, ObjectKind::Note, 1);

            let mut relationships = TestRelationships::new();
            relationships.insert_object(root_object).unwrap();
            relationships.insert_object(binding).unwrap();
            relationships.insert_object(target).unwrap();
            relationships
                .add_relationship(ObjectRelationship::new(
                    root,
                    RelationshipKind::NameBinding,
                    BINDING_OBJECT_ID,
                ))
                .unwrap();
            relationships
                .add_relationship(ObjectRelationship::new(
                    BINDING_OBJECT_ID,
                    RelationshipKind::BindingTarget,
                    TARGET_OBJECT_ID,
                ))
                .unwrap();

            let mut revisions = TestRevisions::new();
            revisions.create_object(root_object, 1, caller).unwrap();
            revisions.create_object(binding, 2, caller).unwrap();
            revisions.create_object(target, 3, caller).unwrap();

            let mut capabilities = CapabilityTable::new();
            let root_capability = capabilities
                .grant(
                    caller,
                    ResourceId::new(root.raw()),
                    RightsMask::new(RightsMask::READ),
                )
                .unwrap();
            let target_capability = capabilities
                .grant(
                    caller,
                    ResourceId::new(TARGET_OBJECT_ID.raw()),
                    RightsMask::new(RightsMask::READ),
                )
                .unwrap();

            Self {
                capabilities,
                relationships,
                revisions,
                caller,
                root,
                root_capability,
                target_capability,
            }
        }

        fn request(&self) -> ObjectLocatorRequest {
            ObjectLocatorRequest::new(
                self.caller,
                self.root,
                self.root_capability,
                self.target_capability,
                RightsMask::new(RightsMask::READ),
            )
        }
    }

    #[test]
    fn invalid_navigation_segment_is_rejected_by_grammar() {
        assert_eq!(
            validate_locator("notes/../secret"),
            Err(LocatorSyntaxError::NavigationSegment)
        );
        assert_eq!(
            validate_locator("."),
            Err(LocatorSyntaxError::NavigationSegment)
        );
        assert_eq!(
            validate_locator("notes//secret"),
            Err(LocatorSyntaxError::EmptySegment)
        );
    }

    #[test]
    fn host_uri_wildcard_and_shell_syntax_are_rejected_by_grammar() {
        assert_eq!(
            validate_locator("/notes"),
            Err(LocatorSyntaxError::HostAbsolute)
        );
        assert_eq!(
            validate_locator("C:/notes"),
            Err(LocatorSyntaxError::DrivePrefix)
        );
        assert_eq!(
            validate_locator("file:notes"),
            Err(LocatorSyntaxError::UriScheme)
        );
        assert_eq!(validate_locator("note*"), Err(LocatorSyntaxError::Wildcard));
        assert_eq!(
            validate_locator("$notes"),
            Err(LocatorSyntaxError::ShellExpansion)
        );
    }

    #[test]
    fn valid_locator_resolves_to_typed_identity_revision_and_relationship_path() {
        let fixture = LocatorFixture::new();

        let resolved = resolve_locator(
            &fixture.capabilities,
            &fixture.relationships,
            &fixture.revisions,
            fixture.request(),
            "notes",
        )
        .unwrap();

        assert_eq!(resolved.object_id(), TARGET_OBJECT_ID);
        assert_eq!(resolved.object_kind(), ObjectKind::Note);
        assert_eq!(resolved.revision(), 1);
        assert_eq!(resolved.relationship_count(), 2);
        assert_eq!(
            resolved.relationship(0).unwrap().kind(),
            RelationshipKind::NameBinding
        );
        assert_eq!(
            resolved.relationship(1).unwrap().kind(),
            RelationshipKind::BindingTarget
        );
    }

    #[test]
    fn multi_segment_locator_uses_separate_traversal_authority_per_boundary() {
        let mut identities = ServiceIdentityTable::new();
        let caller = identities.register_task(TaskId::new(213)).unwrap();
        let root = ObjectId::new(SHELL_WORKSPACE_OBJECT_ID);
        let root_object = TypedObjectRecord::new(root, ObjectKind::WorkspaceSession, 1);
        let first_binding = name_binding_record(BINDING_OBJECT_ID, b"notes");
        let child_namespace =
            TypedObjectRecord::new(CHILD_NAMESPACE_OBJECT_ID, ObjectKind::WorkspaceSession, 1);
        let second_binding = name_binding_record(CHILD_BINDING_OBJECT_ID, b"today");
        let target = TypedObjectRecord::new(CHILD_TARGET_OBJECT_ID, ObjectKind::Note, 1);

        let mut relationships = MultiSegmentRelationships::new();
        relationships.insert_object(root_object).unwrap();
        relationships.insert_object(first_binding).unwrap();
        relationships.insert_object(child_namespace).unwrap();
        relationships.insert_object(second_binding).unwrap();
        relationships.insert_object(target).unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                root,
                RelationshipKind::NameBinding,
                BINDING_OBJECT_ID,
            ))
            .unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                BINDING_OBJECT_ID,
                RelationshipKind::BindingTarget,
                CHILD_NAMESPACE_OBJECT_ID,
            ))
            .unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                CHILD_NAMESPACE_OBJECT_ID,
                RelationshipKind::NameBinding,
                CHILD_BINDING_OBJECT_ID,
            ))
            .unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                CHILD_BINDING_OBJECT_ID,
                RelationshipKind::BindingTarget,
                CHILD_TARGET_OBJECT_ID,
            ))
            .unwrap();

        let mut revisions = MultiSegmentRevisions::new();
        revisions.create_object(root_object, 1, caller).unwrap();
        revisions.create_object(first_binding, 2, caller).unwrap();
        revisions.create_object(child_namespace, 3, caller).unwrap();
        revisions.create_object(second_binding, 4, caller).unwrap();
        revisions.create_object(target, 5, caller).unwrap();

        let mut capabilities = CapabilityTable::new();
        let root_capability = capabilities
            .grant(
                caller,
                ResourceId::new(root.raw()),
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let child_capability = capabilities
            .grant(
                caller,
                ResourceId::new(CHILD_NAMESPACE_OBJECT_ID.raw()),
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let target_capability = capabilities
            .grant(
                caller,
                ResourceId::new(CHILD_TARGET_OBJECT_ID.raw()),
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();

        let mut request = ObjectLocatorRequest::new(
            caller,
            root,
            root_capability,
            target_capability,
            RightsMask::new(RightsMask::READ),
        );
        assert!(request.set_traversal_authority(1, child_capability));
        assert!(!request.set_traversal_authority(MAX_LOCATOR_SEGMENTS, child_capability));

        let resolved = resolve_locator(
            &capabilities,
            &relationships,
            &revisions,
            request,
            "notes/today",
        )
        .unwrap();

        assert_eq!(resolved.object_id(), CHILD_TARGET_OBJECT_ID);
        assert_eq!(resolved.object_kind(), ObjectKind::Note);
        assert_eq!(resolved.relationship_count(), 4);
    }

    #[test]
    fn traversal_authority_is_checked_before_binding_lookup() {
        let fixture = LocatorFixture::new();
        let request = ObjectLocatorRequest::new(
            fixture.caller,
            fixture.root,
            fixture.target_capability,
            fixture.target_capability,
            RightsMask::new(RightsMask::READ),
        );

        assert_eq!(
            resolve_locator(
                &fixture.capabilities,
                &fixture.relationships,
                &fixture.revisions,
                request,
                "notes",
            ),
            Err(ObjectLocatorError::TraversalAuthorityDenied(
                CapabilityError::WrongResource
            ))
        );
    }

    #[test]
    fn final_object_authority_is_checked_after_resolution() {
        let fixture = LocatorFixture::new();
        let request = ObjectLocatorRequest::new(
            fixture.caller,
            fixture.root,
            fixture.root_capability,
            fixture.root_capability,
            RightsMask::new(RightsMask::READ),
        );

        assert_eq!(
            resolve_locator(
                &fixture.capabilities,
                &fixture.relationships,
                &fixture.revisions,
                request,
                "notes",
            ),
            Err(ObjectLocatorError::FinalObjectAuthorityDenied(
                CapabilityError::WrongResource
            ))
        );
    }

    #[test]
    fn adversarial_self_test_is_wired_for_boot_contract() {
        run_adversarial_self_test().unwrap();
    }

    #[test]
    fn invalid_locator_syntax_is_classified_before_graph_or_authority_state() {
        let relationships = TestRelationships::new();
        let revisions = TestRevisions::new();
        let capabilities = CapabilityTable::new();
        let caller = ServiceId::from_raw(41);
        let root = ObjectId::new(SHELL_WORKSPACE_OBJECT_ID);
        let root_capability = CapabilityHandle::from_parts(1, 1);
        let final_authority = CapabilityHandle::from_parts(2, 1);
        let request = ObjectLocatorRequest::new(
            caller,
            root,
            root_capability,
            final_authority,
            RightsMask::new(RightsMask::READ),
        );

        assert_eq!(
            resolve_locator(
                &capabilities,
                &relationships,
                &revisions,
                request,
                "notes/../secret",
            ),
            Err(ObjectLocatorError::Syntax(
                LocatorSyntaxError::NavigationSegment
            ))
        );
        assert_eq!(
            resolve_locator(
                &capabilities,
                &relationships,
                &revisions,
                request,
                "notes//secret",
            ),
            Err(ObjectLocatorError::Syntax(LocatorSyntaxError::EmptySegment))
        );
        assert_eq!(
            resolve_locator(&capabilities, &relationships, &revisions, request, "/notes",),
            Err(ObjectLocatorError::Syntax(LocatorSyntaxError::HostAbsolute))
        );
    }

    #[test]
    fn stale_binding_is_distinct_from_missing_segment() {
        let mut identities = ServiceIdentityTable::new();
        let caller = identities.register_task(TaskId::new(214)).unwrap();
        let root = ObjectId::new(SHELL_WORKSPACE_OBJECT_ID);
        let root_object = TypedObjectRecord::new(root, ObjectKind::WorkspaceSession, 1);
        let binding = name_binding_record(BINDING_OBJECT_ID, b"notes");
        let target = TypedObjectRecord::new(TARGET_OBJECT_ID, ObjectKind::Note, 1);

        let mut relationships = TestRelationships::new();
        relationships.insert_object(root_object).unwrap();
        relationships.insert_object(binding).unwrap();
        relationships.insert_object(target).unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                root,
                RelationshipKind::NameBinding,
                BINDING_OBJECT_ID,
            ))
            .unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                BINDING_OBJECT_ID,
                RelationshipKind::BindingTarget,
                TARGET_OBJECT_ID,
            ))
            .unwrap();

        let mut revisions = TestRevisions::new();
        revisions.create_object(root_object, 1, caller).unwrap();
        revisions.create_object(target, 2, caller).unwrap();

        let mut capabilities = CapabilityTable::new();
        let root_capability = capabilities
            .grant(
                caller,
                ResourceId::new(root.raw()),
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let target_capability = capabilities
            .grant(
                caller,
                ResourceId::new(TARGET_OBJECT_ID.raw()),
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let request = ObjectLocatorRequest::new(
            caller,
            root,
            root_capability,
            target_capability,
            RightsMask::new(RightsMask::READ),
        );

        assert_eq!(
            resolve_locator(&capabilities, &relationships, &revisions, request, "notes",),
            Err(ObjectLocatorError::StaleBinding)
        );

        let fixture = LocatorFixture::new();
        assert_eq!(
            resolve_locator(
                &fixture.capabilities,
                &fixture.relationships,
                &fixture.revisions,
                fixture.request(),
                "missing",
            ),
            Err(ObjectLocatorError::MissingSegment)
        );
    }

    #[test]
    fn adversarial_missing_authorities_are_explicit_denials() {
        let mut identities = ServiceIdentityTable::new();
        let caller = identities.register_task(TaskId::new(215)).unwrap();
        let root = ObjectId::new(SHELL_WORKSPACE_OBJECT_ID);
        let root_object = TypedObjectRecord::new(root, ObjectKind::WorkspaceSession, 1);
        let first_binding = name_binding_record(BINDING_OBJECT_ID, b"notes");
        let child_namespace =
            TypedObjectRecord::new(CHILD_NAMESPACE_OBJECT_ID, ObjectKind::WorkspaceSession, 1);
        let second_binding = name_binding_record(CHILD_BINDING_OBJECT_ID, b"today");
        let target = TypedObjectRecord::new(CHILD_TARGET_OBJECT_ID, ObjectKind::Note, 1);

        let mut relationships = MultiSegmentRelationships::new();
        relationships.insert_object(root_object).unwrap();
        relationships.insert_object(first_binding).unwrap();
        relationships.insert_object(child_namespace).unwrap();
        relationships.insert_object(second_binding).unwrap();
        relationships.insert_object(target).unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                root,
                RelationshipKind::NameBinding,
                BINDING_OBJECT_ID,
            ))
            .unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                BINDING_OBJECT_ID,
                RelationshipKind::BindingTarget,
                CHILD_NAMESPACE_OBJECT_ID,
            ))
            .unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                CHILD_NAMESPACE_OBJECT_ID,
                RelationshipKind::NameBinding,
                CHILD_BINDING_OBJECT_ID,
            ))
            .unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                CHILD_BINDING_OBJECT_ID,
                RelationshipKind::BindingTarget,
                CHILD_TARGET_OBJECT_ID,
            ))
            .unwrap();

        let mut revisions = MultiSegmentRevisions::new();
        revisions.create_object(root_object, 1, caller).unwrap();
        revisions.create_object(first_binding, 2, caller).unwrap();
        revisions.create_object(child_namespace, 3, caller).unwrap();
        revisions.create_object(second_binding, 4, caller).unwrap();
        revisions.create_object(target, 5, caller).unwrap();

        let mut capabilities = CapabilityTable::new();
        let root_capability = capabilities
            .grant(
                caller,
                ResourceId::new(root.raw()),
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let target_capability = capabilities
            .grant(
                caller,
                ResourceId::new(CHILD_TARGET_OBJECT_ID.raw()),
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let request = ObjectLocatorRequest::new(
            caller,
            root,
            root_capability,
            target_capability,
            RightsMask::new(RightsMask::READ),
        );

        assert_eq!(
            resolve_locator(
                &capabilities,
                &relationships,
                &revisions,
                request,
                "notes/today",
            ),
            Err(ObjectLocatorError::MissingTraversalAuthority)
        );

        let fixture = LocatorFixture::new();
        let final_missing_request = ObjectLocatorRequest::new(
            fixture.caller,
            fixture.root,
            fixture.root_capability,
            CapabilityHandle::from_parts(99, 1),
            RightsMask::new(RightsMask::READ),
        );
        assert_eq!(
            resolve_locator(
                &fixture.capabilities,
                &fixture.relationships,
                &fixture.revisions,
                final_missing_request,
                "notes",
            ),
            Err(ObjectLocatorError::FinalObjectAuthorityDenied(
                CapabilityError::InvalidHandle
            ))
        );
    }

    #[test]
    fn duplicate_name_bindings_are_not_resolved_by_order() {
        let mut identities = ServiceIdentityTable::new();
        let caller = identities.register_task(TaskId::new(216)).unwrap();
        let root = ObjectId::new(SHELL_WORKSPACE_OBJECT_ID);
        let root_object = TypedObjectRecord::new(root, ObjectKind::WorkspaceSession, 1);
        let first_binding = name_binding_record(BINDING_OBJECT_ID, b"notes");
        let second_binding = name_binding_record(SECOND_BINDING_OBJECT_ID, b"notes");
        let first_target = TypedObjectRecord::new(TARGET_OBJECT_ID, ObjectKind::Note, 1);
        let second_target = TypedObjectRecord::new(SECOND_TARGET_OBJECT_ID, ObjectKind::Note, 1);

        let mut relationships = AdversarialRelationships::new();
        relationships.insert_object(root_object).unwrap();
        relationships.insert_object(first_binding).unwrap();
        relationships.insert_object(second_binding).unwrap();
        relationships.insert_object(first_target).unwrap();
        relationships.insert_object(second_target).unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                root,
                RelationshipKind::NameBinding,
                BINDING_OBJECT_ID,
            ))
            .unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                BINDING_OBJECT_ID,
                RelationshipKind::BindingTarget,
                TARGET_OBJECT_ID,
            ))
            .unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                root,
                RelationshipKind::NameBinding,
                SECOND_BINDING_OBJECT_ID,
            ))
            .unwrap();
        relationships
            .add_relationship(ObjectRelationship::new(
                SECOND_BINDING_OBJECT_ID,
                RelationshipKind::BindingTarget,
                SECOND_TARGET_OBJECT_ID,
            ))
            .unwrap();

        let mut revisions = AdversarialRevisions::new();
        revisions.create_object(root_object, 1, caller).unwrap();
        revisions.create_object(first_binding, 2, caller).unwrap();
        revisions.create_object(second_binding, 3, caller).unwrap();

        let mut capabilities = CapabilityTable::new();
        let root_capability = capabilities
            .grant(
                caller,
                ResourceId::new(root.raw()),
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let target_capability = capabilities
            .grant(
                caller,
                ResourceId::new(TARGET_OBJECT_ID.raw()),
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let request = ObjectLocatorRequest::new(
            caller,
            root,
            root_capability,
            target_capability,
            RightsMask::new(RightsMask::READ),
        );

        assert_eq!(
            resolve_locator(&capabilities, &relationships, &revisions, request, "notes"),
            Err(ObjectLocatorError::NameCollision)
        );
    }

    #[test]
    fn link_confusion_and_global_root_assumptions_are_denied() {
        let fixture = LocatorFixture::new();
        let mut relationship_confusion = TestRelationships::new();
        let root = fixture.root;
        let root_object = TypedObjectRecord::new(root, ObjectKind::WorkspaceSession, 1);
        let target = TypedObjectRecord::new(TARGET_OBJECT_ID, ObjectKind::Note, 1);
        relationship_confusion.insert_object(root_object).unwrap();
        relationship_confusion.insert_object(target).unwrap();
        relationship_confusion
            .add_relationship(ObjectRelationship::new(
                root,
                RelationshipKind::BelongsTo,
                TARGET_OBJECT_ID,
            ))
            .unwrap();
        let mut revisions = TestRevisions::new();
        revisions
            .create_object(root_object, 1, fixture.caller)
            .unwrap();
        revisions.create_object(target, 2, fixture.caller).unwrap();

        assert_eq!(
            resolve_locator(
                &fixture.capabilities,
                &relationship_confusion,
                &revisions,
                fixture.request(),
                "notes",
            ),
            Err(ObjectLocatorError::MissingSegment)
        );

        let alternate_root =
            TypedObjectRecord::new(ALTERNATE_ROOT_OBJECT_ID, ObjectKind::WorkspaceSession, 1);
        let mut global_relationships = AdversarialRelationships::new();
        global_relationships.insert_object(root_object).unwrap();
        global_relationships.insert_object(alternate_root).unwrap();
        global_relationships
            .insert_object(name_binding_record(BINDING_OBJECT_ID, b"notes"))
            .unwrap();
        global_relationships.insert_object(target).unwrap();
        global_relationships
            .add_relationship(ObjectRelationship::new(
                root,
                RelationshipKind::NameBinding,
                BINDING_OBJECT_ID,
            ))
            .unwrap();
        global_relationships
            .add_relationship(ObjectRelationship::new(
                BINDING_OBJECT_ID,
                RelationshipKind::BindingTarget,
                TARGET_OBJECT_ID,
            ))
            .unwrap();
        let mut global_revisions = AdversarialRevisions::new();
        global_revisions
            .create_object(root_object, 1, fixture.caller)
            .unwrap();
        global_revisions
            .create_object(alternate_root, 2, fixture.caller)
            .unwrap();
        global_revisions
            .create_object(
                name_binding_record(BINDING_OBJECT_ID, b"notes"),
                3,
                fixture.caller,
            )
            .unwrap();
        global_revisions
            .create_object(target, 4, fixture.caller)
            .unwrap();

        let mut capabilities = fixture.capabilities;
        let alternate_root_capability = capabilities
            .grant(
                fixture.caller,
                ResourceId::new(ALTERNATE_ROOT_OBJECT_ID.raw()),
                RightsMask::new(RightsMask::READ),
            )
            .unwrap();
        let alternate_request = ObjectLocatorRequest::new(
            fixture.caller,
            ALTERNATE_ROOT_OBJECT_ID,
            alternate_root_capability,
            fixture.target_capability,
            RightsMask::new(RightsMask::READ),
        );

        assert_eq!(
            resolve_locator(
                &capabilities,
                &global_relationships,
                &global_revisions,
                alternate_request,
                "notes",
            ),
            Err(ObjectLocatorError::MissingSegment)
        );
    }
}
