//! Authoritative task-state adapter over the retained object service.

use crate::{
    dynamic_object_store::MAX_DYNAMIC_OBJECTS,
    object_service::{ObjectService, ObjectServiceError},
    process_context::ActiveUserProcess,
    service_identity::ServiceId,
    shell_objects::{ObjectId, ObjectKind},
    task_context::{self, TaskContextEvent, TaskFingerprint},
    typed_object_format::{ObjectFormatError, TypedObjectField, TypedObjectRecord},
};
use pythos_shared::object_shell_abi::PackedCapability;
pub use pythos_shared::task_abi::{
    TASK_RIGHT_APPEND_EVENT, TASK_RIGHT_APPROVE_PROPOSAL, TASK_RIGHT_CONTROL_STATE,
    TASK_RIGHT_CREATE_PROPOSAL, TASK_RIGHT_READ_CONTEXT, TaskContextSummary, TaskProposalKind,
    TaskProposalListEntry, TaskStatus,
};

const TASK_SERVICE_RESOURCE_RAW: u64 = 0x5453_4B53_5643_0001;
const USER_TASK_RIGHTS: u64 = TASK_RIGHT_READ_CONTEXT
    | TASK_RIGHT_APPEND_EVENT
    | TASK_RIGHT_APPROVE_PROPOSAL
    | TASK_RIGHT_CONTROL_STATE;
const STEWARD_TASK_RIGHTS: u64 = TASK_RIGHT_READ_CONTEXT | TASK_RIGHT_CREATE_PROPOSAL;
const STEWARD_SERVICE_ID: ServiceId = ServiceId::from_raw(0x5059_5453_5445_5701);
const TASK_STEWARD_PRINCIPAL_ID: u64 = 0x5059_5448_5354_0001;
const TASK_STEWARD_PROGRAM_DIGEST: u64 = 0x5453_5445_5741_5244;
const TASK_CAPABILITY_TOKEN_TAG: u64 = 0x8000_0000_0000_0000;
const TASK_CAPABILITY_USER_DOMAIN: u64 = 0x5453_4B55_5345_5201;
const TASK_CAPABILITY_STEWARD_DOMAIN: u64 = 0x5453_4B53_5457_4401;

const TASK_ID_BASE: u64 = 3000;
const PROPOSAL_ID_BASE: u64 = 4000;
const EVENT_ID_BASE: u64 = 5000;
const RELATION_ID_BASE: u64 = 6000;
const RELEVANCE_ASSERTION_ID_BASE: u64 = 7000;
const ID_SPACE: u64 = 1000;

const FIELD_TASK_STATUS: u16 = 1;
const FIELD_TASK_TITLE_HASH: u16 = 2;
const FIELD_TASK_FINGERPRINT: u16 = 3;
const FIELD_PROPOSAL_META: u16 = 1;
const FIELD_PROPOSAL_TARGET_TASK: u16 = 2;
const FIELD_PROPOSAL_CANDIDATE_TASK: u16 = 3;
const FIELD_PROPOSAL_TITLE_REASON_HASH: u16 = 4;
const FIELD_EVENT_META: u16 = 1;
const FIELD_EVENT_TASK: u16 = 2;
const FIELD_EVENT_PROPOSAL: u16 = 3;
const FIELD_EVENT_CONTEXT: u16 = 4;
const FIELD_RELATION_SOURCE: u16 = 1;
const FIELD_RELATION_TARGET: u16 = 2;
const FIELD_RELATION_KIND: u16 = 3;
const FIELD_RELEVANCE_DOMINANCE: u16 = 1;
const FIELD_RELEVANCE_SCORE: u16 = 2;
const FIELD_RELEVANCE_TASKS: u16 = 3;
const FIELD_RELEVANCE_SOURCES_HEAD: u16 = 4;

const PROPOSAL_STATUS_PENDING: u16 = 1;
const PROPOSAL_STATUS_APPROVED: u16 = 2;
const PROPOSAL_STATUS_REJECTED: u16 = 3;

const EVENT_TASK_CREATED: u16 = 1;
const EVENT_PROPOSAL_CREATED: u16 = 2;
const EVENT_PROPOSAL_APPROVED: u16 = 3;
const EVENT_TASK_SUSPENDED: u16 = 4;
const EVENT_TASK_REVIVED: u16 = 5;
const EVENT_TASK_COMPLETED: u16 = 6;
const EVENT_TASK_ABANDONED: u16 = 7;
const EVENT_PROPOSAL_REJECTED: u16 = 8;
const EVENT_TASK_APPENDED: u16 = 9;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskServiceError {
    Denied,
    NotFound,
    ProposalNotPending,
    BadRequest,
    Object(ObjectServiceError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskCreateResult {
    pub task_id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskProposalResult {
    pub proposal_id: u64,
}

pub struct TaskService<'a> {
    objects: &'a mut ObjectService,
    authority: &'a mut TaskAuthorityState,
}

impl<'a> TaskService<'a> {
    pub fn new(
        objects: &'a mut ObjectService,
        authority: &'a mut TaskAuthorityState,
    ) -> Result<Self, TaskServiceError> {
        Ok(Self { objects, authority })
    }

    pub fn create_task(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        title: &[u8],
    ) -> Result<TaskCreateResult, TaskServiceError> {
        self.validate(caller, authority, TASK_RIGHT_CONTROL_STATE)?;
        let mut staged = *self.objects;
        let title_hash = stable_hash(title);
        if let Some(active_task_id) = active_task_id_in(&staged) {
            set_task_status_on(&mut staged, caller, active_task_id, TaskStatus::Suspended)?;
            append_event_on(
                &mut staged,
                caller,
                EVENT_TASK_SUSPENDED,
                active_task_id,
                0,
                None,
            )?;
        }
        let task_id = allocate_object_id(&staged, TASK_ID_BASE);
        staged.create_task_service_object(
            caller,
            task_record(
                task_id,
                TaskStatus::Active,
                title_hash,
                default_task_fingerprint(title_hash),
            )?,
        )?;
        append_event_on(&mut staged, caller, EVENT_TASK_CREATED, task_id, 0, None)?;
        *self.objects = staged;
        Ok(TaskCreateResult { task_id })
    }

    pub fn create_proposal(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        kind: TaskProposalKind,
        target_task_id: u64,
        candidate_task_id: u64,
        score: u64,
        title: &[u8],
        reason: &[u8],
    ) -> Result<TaskProposalResult, TaskServiceError> {
        self.validate(caller, authority, TASK_RIGHT_CREATE_PROPOSAL)?;
        let mut staged = *self.objects;
        let proposal_id = allocate_object_id(&staged, PROPOSAL_ID_BASE);
        staged.create_task_service_object(
            caller,
            proposal_record(
                proposal_id,
                PROPOSAL_STATUS_PENDING,
                kind,
                target_task_id,
                candidate_task_id,
                score,
                stable_hash(title),
                stable_hash(reason),
            )?,
        )?;
        append_event_on(
            &mut staged,
            caller,
            EVENT_PROPOSAL_CREATED,
            target_task_id,
            proposal_id,
            None,
        )?;
        *self.objects = staged;
        Ok(TaskProposalResult { proposal_id })
    }

    pub fn approve_proposal(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        proposal_id: u64,
        suspend_current: bool,
    ) -> Result<TaskCreateResult, TaskServiceError> {
        self.validate(
            caller,
            authority,
            TASK_RIGHT_APPROVE_PROPOSAL | TASK_RIGHT_CONTROL_STATE,
        )?;
        let mut staged = *self.objects;
        let proposal = stored_proposal(&staged, proposal_id)?;
        if proposal.status != PROPOSAL_STATUS_PENDING {
            return Err(TaskServiceError::ProposalNotPending);
        }

        let task_id = match proposal.kind {
            TaskProposalKind::Continuation => {
                let resumed = if proposal.candidate_task_id == 0 {
                    proposal.target_task_id
                } else {
                    proposal.candidate_task_id
                };
                set_task_status_on(&mut staged, caller, resumed, TaskStatus::Active)?;
                resumed
            }
            _ => {
                let created = allocate_object_id(&staged, TASK_ID_BASE);
                staged.create_task_service_object(
                    caller,
                    task_record(
                        created,
                        TaskStatus::Active,
                        proposal.title_hash,
                        proposed_task_fingerprint(proposal.title_hash),
                    )?,
                )?;
                created
            }
        };

        if suspend_current
            && let Some(active_task_id) = active_task_id_in(self.objects)
            && active_task_id != task_id
        {
            set_task_status_on(&mut staged, caller, active_task_id, TaskStatus::Suspended)?;
            append_event_on(
                &mut staged,
                caller,
                EVENT_TASK_SUSPENDED,
                active_task_id,
                proposal_id,
                None,
            )?;
        }
        staged.create_task_service_object(
            caller,
            relation_record(
                allocate_object_id(&staged, RELATION_ID_BASE),
                task_id,
                proposal.target_task_id,
                proposal.kind,
            )?,
        )?;
        staged.revise_task_service_object(
            caller,
            proposal_record(
                proposal_id,
                PROPOSAL_STATUS_APPROVED,
                proposal.kind,
                proposal.target_task_id,
                proposal.candidate_task_id,
                proposal.score,
                proposal.title_hash,
                proposal.reason_hash,
            )?,
        )?;
        append_event_on(
            &mut staged,
            caller,
            EVENT_PROPOSAL_APPROVED,
            task_id,
            proposal_id,
            None,
        )?;
        *self.objects = staged;
        Ok(TaskCreateResult { task_id })
    }

    pub fn suspend_task(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        task_id: u64,
    ) -> Result<(), TaskServiceError> {
        self.transition_task(caller, authority, task_id, TaskStatus::Suspended)
    }

    pub fn revive_task(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        task_id: u64,
    ) -> Result<(), TaskServiceError> {
        self.transition_task(caller, authority, task_id, TaskStatus::Active)
    }

    pub fn complete_task(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        task_id: u64,
    ) -> Result<(), TaskServiceError> {
        self.transition_task(caller, authority, task_id, TaskStatus::Completed)
    }

    pub fn abandon_task(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        task_id: u64,
    ) -> Result<(), TaskServiceError> {
        self.transition_task(caller, authority, task_id, TaskStatus::Abandoned)
    }

    pub fn append_task_event(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        task_id: u64,
    ) -> Result<(), TaskServiceError> {
        self.append_task_event_with_context(caller, authority, task_id, None)
            .map(|_| ())
    }

    pub fn append_task_context_event(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        task_id: u64,
        event: TaskContextEvent,
    ) -> Result<u64, TaskServiceError> {
        self.append_task_event_with_context(caller, authority, task_id, Some(event))
    }

    fn append_task_event_with_context(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        task_id: u64,
        context: Option<TaskContextEvent>,
    ) -> Result<u64, TaskServiceError> {
        self.validate(caller, authority, TASK_RIGHT_APPEND_EVENT)?;
        let mut staged = *self.objects;
        if task_status_in(&staged, task_id).is_none() {
            return Err(TaskServiceError::NotFound);
        }
        let event_id = append_event_on(
            &mut staged,
            caller,
            EVENT_TASK_APPENDED,
            task_id,
            0,
            context,
        )?;
        *self.objects = staged;
        Ok(event_id)
    }

    pub fn reject_proposal(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        proposal_id: u64,
    ) -> Result<(), TaskServiceError> {
        self.validate(caller, authority, TASK_RIGHT_APPROVE_PROPOSAL)?;
        let mut staged = *self.objects;
        let proposal = stored_proposal(&staged, proposal_id)?;
        if proposal.status != PROPOSAL_STATUS_PENDING {
            return Err(TaskServiceError::ProposalNotPending);
        }
        staged.revise_task_service_object(
            caller,
            proposal_record(
                proposal_id,
                PROPOSAL_STATUS_REJECTED,
                proposal.kind,
                proposal.target_task_id,
                proposal.candidate_task_id,
                proposal.score,
                proposal.title_hash,
                proposal.reason_hash,
            )?,
        )?;
        append_event_on(
            &mut staged,
            caller,
            EVENT_PROPOSAL_REJECTED,
            proposal.target_task_id,
            proposal_id,
            None,
        )?;
        *self.objects = staged;
        Ok(())
    }

    pub fn list_pending_proposals(
        &self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        output: &mut [TaskProposalListEntry],
    ) -> Result<usize, TaskServiceError> {
        self.validate(caller, authority, TASK_RIGHT_APPROVE_PROPOSAL)?;
        let mut count = 0usize;
        for record in self.objects.task_service_objects().into_iter().flatten() {
            if record.object_kind() != ObjectKind::TaskProposal || count >= output.len() {
                continue;
            }
            let proposal = stored_proposal(self.objects, record.object_id().raw())?;
            if proposal.status != PROPOSAL_STATUS_PENDING {
                continue;
            }
            output[count] = TaskProposalListEntry {
                status: proposal.status,
                proposal_kind: proposal.kind.code(),
                reserved0: 0,
                proposal_id: record.object_id().raw(),
                target_task_id: proposal.target_task_id,
                candidate_task_id: proposal.candidate_task_id,
                score: proposal.score,
            };
            count += 1;
        }
        Ok(count)
    }

    pub fn read_active_task(
        &self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
    ) -> Result<Option<u64>, TaskServiceError> {
        self.validate(caller, authority, TASK_RIGHT_READ_CONTEXT)?;
        Ok(self.active_task_id())
    }

    pub fn read_context_summary(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
    ) -> Result<TaskContextSummary, TaskServiceError> {
        self.validate(caller, authority, TASK_RIGHT_READ_CONTEXT)?;
        let active_task_id = self.active_task_id().unwrap_or(0);
        let active = if active_task_id == 0 {
            default_task_fingerprint(0)
        } else {
            task_fingerprint_in(self.objects, active_task_id)
                .unwrap_or_else(|| default_task_fingerprint(0))
        };
        let (events, event_count) = context_events_in(self.objects);
        let suspended = matching_suspended_task_in(self.objects);
        let mut summary =
            task_context::summarize_context(active, &events[..event_count], suspended);
        summary.active_task_id = active_task_id;

        if caller == steward_process() {
            let mut staged = *self.objects;
            let assertion_id = allocate_object_id(&staged, RELEVANCE_ASSERTION_ID_BASE);
            staged.create_task_service_object(
                caller,
                relevance_assertion_record(assertion_id, summary)?,
            )?;
            *self.objects = staged;
        }

        Ok(summary)
    }

    pub fn active_task_id(&self) -> Option<u64> {
        active_task_id_in(self.objects)
    }

    pub fn task_exists_for_title(&self, title: &[u8]) -> bool {
        task_exists_for_title_in(self.objects, stable_hash(title))
    }

    pub fn task_status(&self, task_id: u64) -> TaskStatus {
        task_status_in(self.objects, task_id).unwrap_or(TaskStatus::Abandoned)
    }

    pub fn relevance_assertion_count(&self) -> u16 {
        object_count_by_kind_in(self.objects, ObjectKind::RelevanceAssertion)
    }

    pub const fn user_task_control_capability(&self) -> PackedCapability {
        self.authority.user_task_control()
    }

    pub const fn steward_proposal_capability(&self) -> PackedCapability {
        self.authority.steward_proposal()
    }

    pub const fn user_caller(&self) -> ActiveUserProcess {
        self.objects.shell_caller()
    }

    pub fn steward_caller(&self) -> ActiveUserProcess {
        steward_process()
    }

    fn transition_task(
        &mut self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        task_id: u64,
        status: TaskStatus,
    ) -> Result<(), TaskServiceError> {
        self.validate(caller, authority, TASK_RIGHT_CONTROL_STATE)?;
        let mut staged = *self.objects;
        set_task_status_on(&mut staged, caller, task_id, status)?;
        let event_kind = match status {
            TaskStatus::Active => EVENT_TASK_REVIVED,
            TaskStatus::Suspended => EVENT_TASK_SUSPENDED,
            TaskStatus::Completed => EVENT_TASK_COMPLETED,
            TaskStatus::Abandoned => EVENT_TASK_ABANDONED,
        };
        append_event_on(&mut staged, caller, event_kind, task_id, 0, None)?;
        *self.objects = staged;
        Ok(())
    }

    fn validate(
        &self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        required_rights: u64,
    ) -> Result<(), TaskServiceError> {
        self.authority.validate(caller, authority, required_rights)
    }
}

#[cfg(test)]
impl TaskService<'static> {
    pub fn new_for_test() -> Self {
        let objects = Box::leak(Box::new(ObjectService::new_for_test()));
        let authority = Box::leak(Box::new(TaskAuthorityState::new(objects.shell_caller())));
        Self::new(objects, authority).unwrap()
    }

    fn issue_capability_for_test(
        &mut self,
        caller: ActiveUserProcess,
        rights: u64,
    ) -> PackedCapability {
        self.authority
            .issue_extra_for_test(caller, rights, TASK_CAPABILITY_USER_DOMAIN)
    }
}

impl From<crate::capabilities::CapabilityError> for TaskServiceError {
    fn from(_error: crate::capabilities::CapabilityError) -> Self {
        Self::Denied
    }
}

impl From<ObjectFormatError> for TaskServiceError {
    fn from(error: ObjectFormatError) -> Self {
        Self::Object(ObjectServiceError::from(error))
    }
}

impl From<ObjectServiceError> for TaskServiceError {
    fn from(error: ObjectServiceError) -> Self {
        match error {
            ObjectServiceError::Denied => Self::Denied,
            ObjectServiceError::NotFound => Self::NotFound,
            error => Self::Object(error),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskAuthorityState {
    user_task_control: IssuedTaskCapability,
    steward_proposal: IssuedTaskCapability,
    extra_issued: IssuedTaskCapability,
    next_issue: u64,
}

impl TaskAuthorityState {
    pub fn new(user_caller: ActiveUserProcess) -> Self {
        let mut state = Self {
            user_task_control: IssuedTaskCapability::empty(),
            steward_proposal: IssuedTaskCapability::empty(),
            extra_issued: IssuedTaskCapability::empty(),
            next_issue: 1,
        };
        let user_task_control =
            state.issue(user_caller, USER_TASK_RIGHTS, TASK_CAPABILITY_USER_DOMAIN);
        let steward_proposal = state.issue(
            steward_process(),
            STEWARD_TASK_RIGHTS,
            TASK_CAPABILITY_STEWARD_DOMAIN,
        );
        state.user_task_control =
            IssuedTaskCapability::new(user_caller, USER_TASK_RIGHTS, user_task_control);
        state.steward_proposal =
            IssuedTaskCapability::new(steward_process(), STEWARD_TASK_RIGHTS, steward_proposal);
        state
    }

    pub const fn user_task_control(&self) -> PackedCapability {
        self.user_task_control.token
    }

    pub const fn steward_proposal(&self) -> PackedCapability {
        self.steward_proposal.token
    }

    fn issue(&mut self, caller: ActiveUserProcess, rights: u64, domain: u64) -> PackedCapability {
        let token = task_capability_token(caller, rights, self.next_issue, domain);
        self.next_issue = self.next_issue.wrapping_add(1);
        token
    }

    fn validate(
        &self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        required_rights: u64,
    ) -> Result<(), TaskServiceError> {
        if self
            .user_task_control
            .validates(caller, authority, required_rights)
            || self
                .steward_proposal
                .validates(caller, authority, required_rights)
            || self
                .extra_issued
                .validates(caller, authority, required_rights)
        {
            Ok(())
        } else {
            Err(TaskServiceError::Denied)
        }
    }

    #[cfg(test)]
    fn issue_extra_for_test(
        &mut self,
        caller: ActiveUserProcess,
        rights: u64,
        domain: u64,
    ) -> PackedCapability {
        let token = self.issue(caller, rights, domain);
        self.extra_issued = IssuedTaskCapability::new(caller, rights, token);
        token
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IssuedTaskCapability {
    token: PackedCapability,
    holder: ServiceId,
    principal_id: u64,
    program_digest: u64,
    rights: u64,
}

impl IssuedTaskCapability {
    const fn empty() -> Self {
        Self {
            token: PackedCapability::from_raw(0),
            holder: ServiceId::invalid(),
            principal_id: 0,
            program_digest: 0,
            rights: 0,
        }
    }

    const fn new(caller: ActiveUserProcess, rights: u64, token: PackedCapability) -> Self {
        Self {
            token,
            holder: caller.service_id(),
            principal_id: caller.principal_id(),
            program_digest: caller.program_digest(),
            rights,
        }
    }

    fn validates(
        self,
        caller: ActiveUserProcess,
        authority: PackedCapability,
        required_rights: u64,
    ) -> bool {
        self.token == authority
            && self.holder == caller.service_id()
            && self.principal_id == caller.principal_id()
            && self.program_digest == caller.program_digest()
            && self.rights & required_rights == required_rights
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StoredProposal {
    status: u16,
    kind: TaskProposalKind,
    target_task_id: u64,
    candidate_task_id: u64,
    score: u64,
    title_hash: u64,
    reason_hash: u64,
}

pub fn steward_process() -> ActiveUserProcess {
    ActiveUserProcess::new(
        STEWARD_SERVICE_ID,
        TASK_STEWARD_PRINCIPAL_ID,
        TASK_STEWARD_PROGRAM_DIGEST,
    )
}

pub fn proposal_kind_from_code(code: u16) -> Option<TaskProposalKind> {
    match code {
        1 => Some(TaskProposalKind::NewTask),
        2 => Some(TaskProposalKind::Continuation),
        3 => Some(TaskProposalKind::Child),
        4 => Some(TaskProposalKind::Branch),
        5 => Some(TaskProposalKind::Related),
        _ => None,
    }
}

fn task_record(
    task_id: u64,
    status: TaskStatus,
    title_hash: u64,
    fingerprint: TaskFingerprint,
) -> Result<TypedObjectRecord, TaskServiceError> {
    let mut record = TypedObjectRecord::new(ObjectId::new(task_id), ObjectKind::Task, 1);
    record.push_field(u16_field(FIELD_TASK_STATUS, status.code())?)?;
    record.push_field(u64_field(FIELD_TASK_TITLE_HASH, title_hash)?)?;
    record.push_field(fingerprint_field(FIELD_TASK_FINGERPRINT, fingerprint)?)?;
    Ok(record)
}

fn proposal_record(
    proposal_id: u64,
    status: u16,
    kind: TaskProposalKind,
    target_task_id: u64,
    candidate_task_id: u64,
    score: u64,
    title_hash: u64,
    reason_hash: u64,
) -> Result<TypedObjectRecord, TaskServiceError> {
    let mut meta = [0; 16];
    write_u16(&mut meta, 0, status);
    write_u16(&mut meta, 2, kind.code());
    write_u64(&mut meta, 8, score);
    let mut hashes = [0; 16];
    write_u64(&mut hashes, 0, title_hash);
    write_u64(&mut hashes, 8, reason_hash);

    let mut record =
        TypedObjectRecord::new(ObjectId::new(proposal_id), ObjectKind::TaskProposal, 1);
    record.push_field(TypedObjectField::new(FIELD_PROPOSAL_META, 1, &meta)?)?;
    record.push_field(u64_field(FIELD_PROPOSAL_TARGET_TASK, target_task_id)?)?;
    record.push_field(u64_field(FIELD_PROPOSAL_CANDIDATE_TASK, candidate_task_id)?)?;
    record.push_field(TypedObjectField::new(
        FIELD_PROPOSAL_TITLE_REASON_HASH,
        1,
        &hashes,
    )?)?;
    Ok(record)
}

fn event_record(
    event_id: u64,
    event_kind: u16,
    task_id: u64,
    proposal_id: u64,
    context: Option<TaskContextEvent>,
) -> Result<TypedObjectRecord, TaskServiceError> {
    let mut record = TypedObjectRecord::new(ObjectId::new(event_id), ObjectKind::TaskEvent, 1);
    record.push_field(u16_field(FIELD_EVENT_META, event_kind)?)?;
    record.push_field(u64_field(FIELD_EVENT_TASK, task_id)?)?;
    record.push_field(u64_field(FIELD_EVENT_PROPOSAL, proposal_id)?)?;
    if let Some(context) = context {
        record.push_field(context_event_field(FIELD_EVENT_CONTEXT, context)?)?;
    }
    Ok(record)
}

fn relation_record(
    relation_id: u64,
    source_task_id: u64,
    target_task_id: u64,
    kind: TaskProposalKind,
) -> Result<TypedObjectRecord, TaskServiceError> {
    let mut record =
        TypedObjectRecord::new(ObjectId::new(relation_id), ObjectKind::TaskRelation, 1);
    record.push_field(u64_field(FIELD_RELATION_SOURCE, source_task_id)?)?;
    record.push_field(u64_field(FIELD_RELATION_TARGET, target_task_id)?)?;
    record.push_field(u16_field(FIELD_RELATION_KIND, kind.code())?)?;
    Ok(record)
}

fn relevance_assertion_record(
    assertion_id: u64,
    summary: TaskContextSummary,
) -> Result<TypedObjectRecord, TaskServiceError> {
    let mut dominance = [0; 16];
    write_u16(&mut dominance, 0, summary.dominant_object_kind);
    write_u16(&mut dominance, 2, summary.dominant_tool_domain);
    write_u16(&mut dominance, 4, summary.proposal_kind);
    write_u16(&mut dominance, 6, summary.event_count);
    write_u16(&mut dominance, 8, summary.active_match_count);
    write_u16(&mut dominance, 10, summary.candidate_match_count);
    write_u16(&mut dominance, 12, summary.tool_domain_changed);

    let mut score = [0; 16];
    write_u64(&mut score, 0, summary.confidence_score);
    write_u64(&mut score, 8, summary.candidate_tag_hash);

    let mut tasks = [0; 16];
    write_u64(&mut tasks, 0, summary.active_task_id);
    write_u64(&mut tasks, 8, summary.matching_suspended_task_id);

    let mut sources = [0; 16];
    write_u64(&mut sources, 0, summary.source_event_ids[0]);
    write_u64(&mut sources, 8, summary.source_event_ids[1]);

    let mut record = TypedObjectRecord::new(
        ObjectId::new(assertion_id),
        ObjectKind::RelevanceAssertion,
        1,
    );
    record.push_field(TypedObjectField::new(
        FIELD_RELEVANCE_DOMINANCE,
        1,
        &dominance,
    )?)?;
    record.push_field(TypedObjectField::new(FIELD_RELEVANCE_SCORE, 1, &score)?)?;
    record.push_field(TypedObjectField::new(FIELD_RELEVANCE_TASKS, 1, &tasks)?)?;
    record.push_field(TypedObjectField::new(
        FIELD_RELEVANCE_SOURCES_HEAD,
        1,
        &sources,
    )?)?;
    Ok(record)
}

fn set_task_status_on(
    objects: &mut ObjectService,
    caller: ActiveUserProcess,
    task_id: u64,
    status: TaskStatus,
) -> Result<(), TaskServiceError> {
    let record = objects
        .task_service_object(ObjectId::new(task_id))
        .ok_or(TaskServiceError::NotFound)?;
    if record.object_kind() != ObjectKind::Task {
        return Err(TaskServiceError::NotFound);
    }
    let title_hash = task_title_hash(record).ok_or(TaskServiceError::BadRequest)?;
    let fingerprint =
        task_fingerprint(record).unwrap_or_else(|| default_task_fingerprint(title_hash));
    objects.revise_task_service_object(
        caller,
        task_record(task_id, status, title_hash, fingerprint)?,
    )?;
    Ok(())
}

fn append_event_on(
    objects: &mut ObjectService,
    caller: ActiveUserProcess,
    event_kind: u16,
    task_id: u64,
    proposal_id: u64,
    context: Option<TaskContextEvent>,
) -> Result<u64, TaskServiceError> {
    let event_id = allocate_object_id(objects, EVENT_ID_BASE);
    objects.create_task_service_object(
        caller,
        event_record(
            event_id,
            event_kind,
            task_id,
            proposal_id,
            context.map(|event| event.with_event_id(event_id)),
        )?,
    )?;
    Ok(event_id)
}

fn active_task_id_in(objects: &ObjectService) -> Option<u64> {
    for record in objects.task_service_objects().into_iter().flatten() {
        if record.object_kind() == ObjectKind::Task
            && task_status(record) == Some(TaskStatus::Active)
        {
            return Some(record.object_id().raw());
        }
    }
    None
}

fn task_status_in(objects: &ObjectService, task_id: u64) -> Option<TaskStatus> {
    objects
        .task_service_object(ObjectId::new(task_id))
        .and_then(task_status)
}

fn task_exists_for_title_in(objects: &ObjectService, title_hash: u64) -> bool {
    objects
        .task_service_objects()
        .into_iter()
        .flatten()
        .any(|record| {
            record.object_kind() == ObjectKind::Task && task_title_hash(record) == Some(title_hash)
        })
}

fn object_count_by_kind_in(objects: &ObjectService, kind: ObjectKind) -> u16 {
    objects
        .task_service_objects()
        .into_iter()
        .flatten()
        .filter(|record| record.object_kind() == kind)
        .count() as u16
}

fn task_fingerprint_in(objects: &ObjectService, task_id: u64) -> Option<TaskFingerprint> {
    objects
        .task_service_object(ObjectId::new(task_id))
        .and_then(task_fingerprint)
}

fn matching_suspended_task_in(objects: &ObjectService) -> Option<(u64, TaskFingerprint)> {
    for record in objects.task_service_objects().into_iter().flatten() {
        if record.object_kind() == ObjectKind::Task
            && task_status(record) == Some(TaskStatus::Suspended)
            && let Some(fingerprint) = task_fingerprint(record)
        {
            return Some((record.object_id().raw(), fingerprint));
        }
    }
    None
}

fn context_events_in(objects: &ObjectService) -> ([TaskContextEvent; MAX_DYNAMIC_OBJECTS], usize) {
    let mut events = [TaskContextEvent::new(0, 0, 0, 0); MAX_DYNAMIC_OBJECTS];
    let mut count = 0;
    for record in objects.task_service_objects().into_iter().flatten() {
        if record.object_kind() == ObjectKind::TaskEvent
            && let Some(event) = context_event(record)
            && count < events.len()
        {
            events[count] = event.with_event_id(record.object_id().raw());
            count += 1;
        }
    }
    (events, count)
}

fn stored_proposal(
    objects: &ObjectService,
    proposal_id: u64,
) -> Result<StoredProposal, TaskServiceError> {
    let record = objects
        .task_service_object(ObjectId::new(proposal_id))
        .ok_or(TaskServiceError::NotFound)?;
    if record.object_kind() != ObjectKind::TaskProposal {
        return Err(TaskServiceError::NotFound);
    }
    let meta = field_value(record, FIELD_PROPOSAL_META).ok_or(TaskServiceError::BadRequest)?;
    let hashes = field_value(record, FIELD_PROPOSAL_TITLE_REASON_HASH)
        .ok_or(TaskServiceError::BadRequest)?;
    Ok(StoredProposal {
        status: read_u16(&meta, 0),
        kind: proposal_kind_from_code(read_u16(&meta, 2)).ok_or(TaskServiceError::BadRequest)?,
        target_task_id: read_u64(
            &field_value(record, FIELD_PROPOSAL_TARGET_TASK).ok_or(TaskServiceError::BadRequest)?,
            0,
        ),
        candidate_task_id: read_u64(
            &field_value(record, FIELD_PROPOSAL_CANDIDATE_TASK)
                .ok_or(TaskServiceError::BadRequest)?,
            0,
        ),
        score: read_u64(&meta, 8),
        title_hash: read_u64(&hashes, 0),
        reason_hash: read_u64(&hashes, 8),
    })
}

fn task_status(record: TypedObjectRecord) -> Option<TaskStatus> {
    field_value(record, FIELD_TASK_STATUS).and_then(|value| match read_u16(&value, 0) {
        1 => Some(TaskStatus::Active),
        2 => Some(TaskStatus::Suspended),
        3 => Some(TaskStatus::Completed),
        4 => Some(TaskStatus::Abandoned),
        _ => None,
    })
}

fn task_title_hash(record: TypedObjectRecord) -> Option<u64> {
    field_value(record, FIELD_TASK_TITLE_HASH).map(|value| read_u64(&value, 0))
}

fn task_fingerprint(record: TypedObjectRecord) -> Option<TaskFingerprint> {
    field_value(record, FIELD_TASK_FINGERPRINT).map(|value| TaskFingerprint {
        object_kind: read_u16(&value, 0),
        tool_domain: read_u16(&value, 2),
        tag_hash: read_u64(&value, 8),
    })
}

fn context_event(record: TypedObjectRecord) -> Option<TaskContextEvent> {
    field_value(record, FIELD_EVENT_CONTEXT).map(|value| {
        TaskContextEvent::new(
            record.object_id().raw(),
            read_u16(&value, 0),
            read_u16(&value, 2),
            read_u64(&value, 8),
        )
        .with_flags(read_u16(&value, 4))
    })
}

fn field_value(record: TypedObjectRecord, field_id: u16) -> Option<[u8; 16]> {
    let mut index = 0;
    while index < record.field_count() {
        if let Some(field) = record.field(index)
            && field.field_id() == field_id
        {
            return Some(field.value());
        }
        index += 1;
    }
    None
}

fn allocate_object_id(objects: &ObjectService, base: u64) -> u64 {
    let mut next = base;
    for record in objects.task_service_objects().into_iter().flatten() {
        let raw = record.object_id().raw();
        if raw >= base && raw < base + ID_SPACE && raw >= next {
            next = raw + 1;
        }
    }
    next
}

fn u16_field(field_id: u16, value: u16) -> Result<TypedObjectField, ObjectFormatError> {
    TypedObjectField::new(field_id, 1, &value.to_le_bytes())
}

fn u64_field(field_id: u16, value: u64) -> Result<TypedObjectField, ObjectFormatError> {
    TypedObjectField::new(field_id, 1, &value.to_le_bytes())
}

fn fingerprint_field(
    field_id: u16,
    fingerprint: TaskFingerprint,
) -> Result<TypedObjectField, ObjectFormatError> {
    let mut value = [0; 16];
    write_u16(&mut value, 0, fingerprint.object_kind);
    write_u16(&mut value, 2, fingerprint.tool_domain);
    write_u64(&mut value, 8, fingerprint.tag_hash);
    TypedObjectField::new(field_id, 1, &value)
}

fn context_event_field(
    field_id: u16,
    event: TaskContextEvent,
) -> Result<TypedObjectField, ObjectFormatError> {
    let mut value = [0; 16];
    write_u16(&mut value, 0, event.object_kind);
    write_u16(&mut value, 2, event.tool_domain);
    write_u16(&mut value, 4, event.flags);
    write_u64(&mut value, 8, event.tag_hash);
    TypedObjectField::new(field_id, 1, &value)
}

fn default_task_fingerprint(title_hash: u64) -> TaskFingerprint {
    TaskFingerprint::new(
        task_context::object_kind_code(ObjectKind::Note),
        task_context::TOOL_DOMAIN_STORAGE,
        title_hash,
    )
}

fn proposed_task_fingerprint(title_hash: u64) -> TaskFingerprint {
    TaskFingerprint::new(
        task_context::object_kind_code(ObjectKind::Task),
        task_context::TOOL_DOMAIN_GRAPH,
        title_hash,
    )
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^ (bytes.len() as u64)
}

fn task_capability_token(
    caller: ActiveUserProcess,
    rights: u64,
    issue: u64,
    domain: u64,
) -> PackedCapability {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for word in [
        domain,
        caller.service_id().raw(),
        caller.principal_id(),
        caller.program_digest(),
        rights,
        issue,
        TASK_SERVICE_RESOURCE_RAW,
    ] {
        for byte in word.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    PackedCapability::from_raw((hash ^ 0x5453_4B43_4150_0001) | TASK_CAPABILITY_TOKEN_TAG)
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
    fn proposal_does_not_change_active_task_until_user_approval() {
        let mut service = TaskService::new_for_test();
        let user = service.user_caller();
        let steward = service.steward_caller();
        let user_control = service.user_task_control_capability();
        let steward_propose = service.steward_proposal_capability();

        let task_a = service
            .create_task(user, user_control, b"Universal Boot")
            .unwrap();
        let proposal = service
            .create_proposal(
                steward,
                steward_propose,
                TaskProposalKind::NewTask,
                task_a.task_id,
                0,
                85,
                b"Semantic Task Runtime",
                b"recent context diverged",
            )
            .unwrap();

        assert_eq!(service.active_task_id(), Some(task_a.task_id));
        assert!(!service.task_exists_for_title(b"Semantic Task Runtime"));

        let task_b = service
            .approve_proposal(user, user_control, proposal.proposal_id, true)
            .unwrap();
        assert_eq!(service.task_status(task_a.task_id), TaskStatus::Suspended);
        assert_eq!(service.task_status(task_b.task_id), TaskStatus::Active);
    }

    #[test]
    fn steward_cannot_create_approve_or_change_task_state() {
        let mut service = TaskService::new_for_test();
        let steward = service.steward_caller();
        let proposal_cap = service.steward_proposal_capability();

        assert_eq!(
            service.create_task(steward, proposal_cap, b"forged"),
            Err(TaskServiceError::Denied)
        );
        assert_eq!(
            service.approve_proposal(steward, proposal_cap, 1, true),
            Err(TaskServiceError::Denied)
        );
        assert_eq!(
            service.suspend_task(steward, proposal_cap, 1),
            Err(TaskServiceError::Denied)
        );
    }

    #[test]
    fn fixed_slot_generation_forgery_is_not_task_authority() {
        let mut service = TaskService::new_for_test();
        let user = service.user_caller();
        let forged = PackedCapability::from_parts(0, 1);

        assert_eq!(
            service.create_task(user, forged, b"forged"),
            Err(TaskServiceError::Denied)
        );
        assert_eq!(service.active_task_id(), None);
    }

    #[test]
    fn generic_pyth_runtime_is_not_the_task_steward() {
        let mut service = TaskService::new_for_test();
        let generic_runtime = ActiveUserProcess::new(
            ServiceId::from_raw(0x5059_5447_5254_0001),
            TASK_STEWARD_PRINCIPAL_ID,
            TASK_STEWARD_PROGRAM_DIGEST,
        );
        let proposal_cap = service.steward_proposal_capability();

        assert_eq!(
            service.create_proposal(
                generic_runtime,
                proposal_cap,
                TaskProposalKind::NewTask,
                0,
                0,
                85,
                b"Semantic Task Runtime",
                b"recent context diverged",
            ),
            Err(TaskServiceError::Denied)
        );
    }

    #[test]
    fn approval_requires_task_control_not_only_proposal_approval() {
        let mut service = TaskService::new_for_test();
        let user = service.user_caller();
        let steward = service.steward_caller();
        let user_control = service.user_task_control_capability();
        let steward_propose = service.steward_proposal_capability();
        let approve_only = service.issue_capability_for_test(user, TASK_RIGHT_APPROVE_PROPOSAL);

        let task_a = service
            .create_task(user, user_control, b"Universal Boot")
            .unwrap();
        let proposal = service
            .create_proposal(
                steward,
                steward_propose,
                TaskProposalKind::NewTask,
                task_a.task_id,
                0,
                85,
                b"Semantic Task Runtime",
                b"recent context diverged",
            )
            .unwrap();

        assert_eq!(
            service.approve_proposal(user, approve_only, proposal.proposal_id, false),
            Err(TaskServiceError::Denied)
        );
        assert_eq!(service.active_task_id(), Some(task_a.task_id));
        assert!(!service.task_exists_for_title(b"Semantic Task Runtime"));
    }

    #[test]
    fn user_can_list_pending_proposals_but_steward_cannot() {
        let mut service = TaskService::new_for_test();
        let user = service.user_caller();
        let steward = service.steward_caller();
        let user_control = service.user_task_control_capability();
        let steward_propose = service.steward_proposal_capability();
        let task_a = service
            .create_task(user, user_control, b"Universal Boot")
            .unwrap();
        let proposal = service
            .create_proposal(
                steward,
                steward_propose,
                TaskProposalKind::NewTask,
                task_a.task_id,
                0,
                85,
                b"Semantic Task Runtime",
                b"recent context diverged",
            )
            .unwrap();
        let mut output = [TaskProposalListEntry {
            status: 0,
            proposal_kind: 0,
            reserved0: 0,
            proposal_id: 0,
            target_task_id: 0,
            candidate_task_id: 0,
            score: 0,
        }; 4];

        assert_eq!(
            service
                .list_pending_proposals(steward, steward_propose, &mut output)
                .unwrap_err(),
            TaskServiceError::Denied
        );
        let count = service
            .list_pending_proposals(user, user_control, &mut output)
            .unwrap();

        assert_eq!(count, 1);
        assert_eq!(output[0].proposal_id, proposal.proposal_id);
        assert_eq!(output[0].target_task_id, task_a.task_id);
        assert_eq!(output[0].score, 85);
    }

    #[test]
    fn task_history_can_cross_legacy_dynamic_object_limit() {
        let mut service = TaskService::new_for_test();
        let user = service.user_caller();
        let steward = service.steward_caller();
        let user_control = service.user_task_control_capability();
        let steward_propose = service.steward_proposal_capability();

        let task_a = service
            .create_task(user, user_control, b"Universal Boot")
            .unwrap();
        let proposal = service
            .create_proposal(
                steward,
                steward_propose,
                TaskProposalKind::NewTask,
                task_a.task_id,
                0,
                85,
                b"Semantic Task Runtime",
                b"recent context diverged",
            )
            .unwrap();
        let task_b = service
            .approve_proposal(user, user_control, proposal.proposal_id, true)
            .unwrap();

        for _ in 0..5 {
            service
                .append_task_context_event(
                    user,
                    user_control,
                    task_b.task_id,
                    TaskContextEvent::new(
                        0,
                        task_context::object_kind_code(ObjectKind::Task),
                        task_context::TOOL_DOMAIN_GRAPH,
                        stable_hash(b"semantic"),
                    ),
                )
                .unwrap();
        }

        let summary = service.read_context_summary(user, user_control).unwrap();
        assert_eq!(summary.active_task_id, task_b.task_id);
        assert!(summary.event_count >= 5);
    }

    #[test]
    fn steward_context_summary_records_relevance_without_task_control() {
        let mut service = TaskService::new_for_test();
        let user = service.user_caller();
        let steward = service.steward_caller();
        let user_control = service.user_task_control_capability();
        let steward_context = service.steward_proposal_capability();
        let task_a = service
            .create_task(user, user_control, b"Universal Boot")
            .unwrap();
        let semantic_tag = stable_hash(b"semantic");

        for _ in 0..8 {
            service
                .append_task_context_event(
                    user,
                    user_control,
                    task_a.task_id,
                    TaskContextEvent::new(
                        0,
                        task_context::object_kind_code(ObjectKind::Task),
                        task_context::TOOL_DOMAIN_GRAPH,
                        semantic_tag,
                    ),
                )
                .unwrap();
        }

        let summary = service
            .read_context_summary(steward, steward_context)
            .unwrap();

        assert_eq!(summary.active_task_id, task_a.task_id);
        assert_eq!(summary.confidence_score, 85);
        assert_eq!(summary.proposal_kind, TaskProposalKind::NewTask.code());
        assert_eq!(summary.source_event_ids, [5001, 5002, 5003, 5004]);
        assert_eq!(service.active_task_id(), Some(task_a.task_id));
        assert_eq!(service.relevance_assertion_count(), 1);
        assert_eq!(
            service.approve_proposal(steward, steward_context, 1, true),
            Err(TaskServiceError::Denied)
        );
    }
}
