//! Deterministic context scoring for the proposal-only Task Steward.

use crate::shell_objects::ObjectKind;
use pythos_shared::package_abi::{
    OBJECT_KIND_PACKAGE, OBJECT_KIND_PACKAGE_DEFINED_OBJECT, OBJECT_KIND_SCHEMA_DEFINITION,
};
use pythos_shared::task_abi::{
    OBJECT_KIND_CAPABILITY_REQUEST, OBJECT_KIND_RELEVANCE_ASSERTION, OBJECT_KIND_TASK,
    OBJECT_KIND_TASK_EVENT, OBJECT_KIND_TASK_PROPOSAL, OBJECT_KIND_TASK_RELATION,
    TaskContextSummary, TaskProposalKind,
};

pub const TOOL_DOMAIN_STORAGE: u16 = 1;
pub const TOOL_DOMAIN_GRAPH: u16 = 2;
pub const EVENT_FLAG_PARENT_OBJECTIVE: u16 = 0x0001;
pub const EVENT_FLAG_ALTERNATIVE_METHOD: u16 = 0x0002;
pub const EVENT_FLAG_SHARED_OBJECT: u16 = 0x0004;
const CONTEXT_WINDOW: usize = 8;
const CHANGE_THRESHOLD: u16 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskFingerprint {
    pub object_kind: u16,
    pub tool_domain: u16,
    pub tag_hash: u64,
}

impl TaskFingerprint {
    pub const fn new(object_kind: u16, tool_domain: u16, tag_hash: u64) -> Self {
        Self {
            object_kind,
            tool_domain,
            tag_hash,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskContextEvent {
    pub event_id: u64,
    pub object_kind: u16,
    pub tool_domain: u16,
    pub tag_hash: u64,
    pub flags: u16,
}

impl TaskContextEvent {
    pub const fn new(event_id: u64, object_kind: u16, tool_domain: u16, tag_hash: u64) -> Self {
        Self {
            event_id,
            object_kind,
            tool_domain,
            tag_hash,
            flags: 0,
        }
    }

    pub const fn with_flags(mut self, flags: u16) -> Self {
        self.flags = flags;
        self
    }

    pub const fn with_event_id(mut self, event_id: u64) -> Self {
        self.event_id = event_id;
        self
    }
}

pub const fn object_kind_code(kind: ObjectKind) -> u16 {
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

pub fn summarize_context(
    active: TaskFingerprint,
    events: &[TaskContextEvent],
    matching_suspended: Option<(u64, TaskFingerprint)>,
) -> TaskContextSummary {
    let window_start = events.len().saturating_sub(CONTEXT_WINDOW);
    let window = &events[window_start..];
    let event_count = window.len() as u16;
    let active_match_count = count_tag(window, active.tag_hash);
    let (candidate_tag_hash, candidate_match_count) = dominant_shift_tag(window, active.tag_hash);
    let (dominant_object_kind, dominant_object_count) =
        dominant_object_kind(window, active.object_kind);
    let (dominant_tool_domain, dominant_tool_count) =
        dominant_tool_domain(window, active.tool_domain);
    let tool_domain_changed = u16::from(
        dominant_tool_domain != active.tool_domain && dominant_tool_count >= CHANGE_THRESHOLD,
    );

    let mut confidence_score = 0u64;
    if candidate_match_count >= CHANGE_THRESHOLD {
        confidence_score += 40;
    }
    if dominant_tool_domain != active.tool_domain && dominant_tool_count >= CHANGE_THRESHOLD {
        confidence_score += 25;
    }
    if dominant_object_kind != active.object_kind && dominant_object_count >= CHANGE_THRESHOLD {
        confidence_score += 20;
    }

    let matching_suspended_task_id = matching_suspended
        .filter(|(_, fingerprint)| {
            candidate_match_count >= CHANGE_THRESHOLD && fingerprint.tag_hash == candidate_tag_hash
        })
        .map_or(0, |(task_id, _)| task_id);
    if matching_suspended_task_id != 0 {
        confidence_score += 15;
    }
    if confidence_score > 100 {
        confidence_score = 100;
    }

    let proposal_kind = if confidence_score >= 70 {
        proposal_kind_for_candidate(window, candidate_tag_hash, matching_suspended_task_id).code()
    } else {
        0
    };

    TaskContextSummary {
        active_task_id: 0,
        matching_suspended_task_id,
        dominant_object_kind,
        dominant_tool_domain,
        proposal_kind,
        event_count,
        active_match_count,
        candidate_match_count,
        tool_domain_changed,
        reserved0: 0,
        confidence_score,
        candidate_tag_hash,
        source_event_ids: source_event_ids(window, candidate_tag_hash),
    }
}

fn dominant_shift_tag(events: &[TaskContextEvent], active_tag_hash: u64) -> (u64, u16) {
    let mut best_tag = 0;
    let mut best_count = 0;
    let mut index = 0;
    while index < events.len() {
        let tag_hash = events[index].tag_hash;
        if tag_hash != active_tag_hash {
            let count = count_tag(events, tag_hash);
            if count > best_count {
                best_tag = tag_hash;
                best_count = count;
            }
        }
        index += 1;
    }
    (best_tag, best_count)
}

fn dominant_object_kind(events: &[TaskContextEvent], active_object_kind: u16) -> (u16, u16) {
    let mut best_kind = active_object_kind;
    let mut best_count = 0;
    let mut index = 0;
    while index < events.len() {
        let object_kind = events[index].object_kind;
        let count = count_object_kind(events, object_kind);
        if count > best_count {
            best_kind = object_kind;
            best_count = count;
        }
        index += 1;
    }
    (best_kind, best_count)
}

fn dominant_tool_domain(events: &[TaskContextEvent], active_tool_domain: u16) -> (u16, u16) {
    let mut best_domain = active_tool_domain;
    let mut best_count = 0;
    let mut index = 0;
    while index < events.len() {
        let tool_domain = events[index].tool_domain;
        let count = count_tool_domain(events, tool_domain);
        if count > best_count {
            best_domain = tool_domain;
            best_count = count;
        }
        index += 1;
    }
    (best_domain, best_count)
}

fn count_tag(events: &[TaskContextEvent], tag_hash: u64) -> u16 {
    events
        .iter()
        .filter(|event| event.tag_hash == tag_hash)
        .count() as u16
}

fn count_object_kind(events: &[TaskContextEvent], object_kind: u16) -> u16 {
    events
        .iter()
        .filter(|event| event.object_kind == object_kind)
        .count() as u16
}

fn count_tool_domain(events: &[TaskContextEvent], tool_domain: u16) -> u16 {
    events
        .iter()
        .filter(|event| event.tool_domain == tool_domain)
        .count() as u16
}

fn proposal_kind_for_candidate(
    events: &[TaskContextEvent],
    candidate_tag_hash: u64,
    matching_suspended_task_id: u64,
) -> TaskProposalKind {
    if matching_suspended_task_id != 0 {
        return TaskProposalKind::Continuation;
    }
    let flags = candidate_flags(events, candidate_tag_hash);
    if flags & EVENT_FLAG_PARENT_OBJECTIVE != 0 {
        TaskProposalKind::Child
    } else if flags & EVENT_FLAG_ALTERNATIVE_METHOD != 0 {
        TaskProposalKind::Branch
    } else if flags & EVENT_FLAG_SHARED_OBJECT != 0 {
        TaskProposalKind::Related
    } else {
        TaskProposalKind::NewTask
    }
}

fn candidate_flags(events: &[TaskContextEvent], candidate_tag_hash: u64) -> u16 {
    let mut flags = 0;
    let mut index = 0;
    while index < events.len() {
        if events[index].tag_hash == candidate_tag_hash {
            flags |= events[index].flags;
        }
        index += 1;
    }
    flags
}

fn source_event_ids(events: &[TaskContextEvent], candidate_tag_hash: u64) -> [u64; 4] {
    let mut ids = [0; 4];
    let mut id_index = 0;
    let mut event_index = 0;
    while event_index < events.len() && id_index < ids.len() {
        let event = events[event_index];
        if event.tag_hash == candidate_tag_hash {
            ids[id_index] = event.event_id;
            id_index += 1;
        }
        event_index += 1;
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(text: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in text {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^ (text.len() as u64)
    }

    fn event(event_id: u64, object_kind: u16, tool_domain: u16, tag_hash: u64) -> TaskContextEvent {
        TaskContextEvent::new(event_id, object_kind, tool_domain, tag_hash)
    }

    #[test]
    fn score_crosses_threshold_only_for_sustained_context_change() {
        let active = TaskFingerprint::new(
            object_kind_code(ObjectKind::Note),
            TOOL_DOMAIN_STORAGE,
            tag(b"universal-boot"),
        );
        let short_shift = [
            event(
                1,
                object_kind_code(ObjectKind::Task),
                TOOL_DOMAIN_GRAPH,
                tag(b"semantic"),
            ),
            event(
                2,
                object_kind_code(ObjectKind::Task),
                TOOL_DOMAIN_GRAPH,
                tag(b"semantic"),
            ),
        ];
        assert!(summarize_context(active, &short_shift, None).confidence_score < 70);

        let sustained = [
            event(
                10,
                object_kind_code(ObjectKind::Task),
                TOOL_DOMAIN_GRAPH,
                tag(b"semantic"),
            ),
            event(
                11,
                object_kind_code(ObjectKind::Task),
                TOOL_DOMAIN_GRAPH,
                tag(b"semantic"),
            ),
            event(
                12,
                object_kind_code(ObjectKind::Task),
                TOOL_DOMAIN_GRAPH,
                tag(b"semantic"),
            ),
            event(
                13,
                object_kind_code(ObjectKind::Task),
                TOOL_DOMAIN_GRAPH,
                tag(b"semantic"),
            ),
            event(
                14,
                object_kind_code(ObjectKind::Task),
                TOOL_DOMAIN_GRAPH,
                tag(b"semantic"),
            ),
            event(
                15,
                object_kind_code(ObjectKind::Task),
                TOOL_DOMAIN_GRAPH,
                tag(b"semantic"),
            ),
            event(
                16,
                object_kind_code(ObjectKind::Task),
                TOOL_DOMAIN_GRAPH,
                tag(b"semantic"),
            ),
            event(
                17,
                object_kind_code(ObjectKind::Task),
                TOOL_DOMAIN_GRAPH,
                tag(b"semantic"),
            ),
        ];
        let summary = summarize_context(active, &sustained, None);
        assert_eq!(summary.confidence_score, 85);
        assert_eq!(summary.proposal_kind, TaskProposalKind::NewTask.code());
        assert_eq!(summary.source_event_ids, [10, 11, 12, 13]);
    }
}
