use crate::types::PythType;
use pythos_shared::pyth_tig::opcode::{
    RESOURCE_GRAPH, RESOURCE_OBJECT, RESOURCE_OBJECT_WORKSPACE, RESOURCE_SYSTEM_LOG, RESOURCE_TASK,
    RIGHTS_APPEND, RIGHTS_CREATE, RIGHTS_QUERY, RIGHTS_READ, RIGHTS_REVISE,
};

const CAP_UTF8: [PythType; 2] = [PythType::Capability, PythType::Utf8];
const CAP_U64: [PythType; 2] = [PythType::Capability, PythType::U64];
const CAP_OBJECT_ID: [PythType; 2] = [PythType::Capability, PythType::ObjectId];
const CAP_OBJECT_U64_UTF8: [PythType; 4] = [
    PythType::Capability,
    PythType::ObjectId,
    PythType::U64,
    PythType::Utf8,
];
const CAP_ONLY: [PythType; 1] = [PythType::Capability];
const TASK_PROPOSE: [PythType; 3] = [PythType::Capability, PythType::TaskId, PythType::U64];
const GRAPH_RELATED: [PythType; 3] = [PythType::Capability, PythType::TaskId, PythType::U64];
const RELEVANCE_EMIT: [PythType; 4] = [
    PythType::Capability,
    PythType::ObjectId,
    PythType::U64,
    PythType::Utf8,
];
const CAPABILITY_REQUEST: [PythType; 3] = [PythType::Capability, PythType::U64, PythType::U64];
const NO_ARGS: [PythType; 0] = [];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Intrinsic {
    SystemLog,
    ObjectCreate,
    ObjectCreatedCapability,
    ObjectCreatedRevision,
    ObjectQuery,
    ObjectQueriedCapability,
    ObjectInspect,
    ObjectInspectedRevision,
    ObjectRevise,
    ObjectHistory,
    TaskActive,
    TaskContextActive,
    TaskContextCandidate,
    TaskContextScore,
    TaskContextKind,
    TaskContextReason,
    TaskPropose,
    GraphRelated,
    RelevanceEmit,
    CapabilityRequest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityRequirement {
    pub resource_kind: u16,
    pub rights: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostProducer {
    ObjectCreate,
    ObjectQuery,
    ObjectInspect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostResultAccess {
    CreatedCapability,
    CreatedRevision,
    QueriedCapability,
    InspectedRevision,
}

impl Intrinsic {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "system.log" => Self::SystemLog,
            "object.create" => Self::ObjectCreate,
            "object.created_capability" => Self::ObjectCreatedCapability,
            "object.created_revision" => Self::ObjectCreatedRevision,
            "object.query" => Self::ObjectQuery,
            "object.queried_capability" => Self::ObjectQueriedCapability,
            "object.inspect" => Self::ObjectInspect,
            "object.inspected_revision" => Self::ObjectInspectedRevision,
            "object.revise" => Self::ObjectRevise,
            "object.history" => Self::ObjectHistory,
            "task.active" => Self::TaskActive,
            "task.context_active" => Self::TaskContextActive,
            "task.context_candidate" => Self::TaskContextCandidate,
            "task.context_score" => Self::TaskContextScore,
            "task.context_kind" => Self::TaskContextKind,
            "task.context_reason" => Self::TaskContextReason,
            "task.propose" => Self::TaskPropose,
            "graph.related" => Self::GraphRelated,
            "relevance.emit" => Self::RelevanceEmit,
            "capability.request" => Self::CapabilityRequest,
            _ => return None,
        })
    }

    pub const fn arg_types(self) -> &'static [PythType] {
        match self {
            Self::SystemLog => &CAP_UTF8,
            Self::ObjectCreate | Self::ObjectQuery => &CAP_U64,
            Self::ObjectCreatedCapability
            | Self::ObjectCreatedRevision
            | Self::ObjectQueriedCapability
            | Self::ObjectInspectedRevision => &NO_ARGS,
            Self::ObjectInspect | Self::ObjectHistory => &CAP_OBJECT_ID,
            Self::ObjectRevise => &CAP_OBJECT_U64_UTF8,
            Self::TaskActive
            | Self::TaskContextActive
            | Self::TaskContextCandidate
            | Self::TaskContextScore
            | Self::TaskContextKind
            | Self::TaskContextReason => &CAP_ONLY,
            Self::TaskPropose => &TASK_PROPOSE,
            Self::GraphRelated => &GRAPH_RELATED,
            Self::RelevanceEmit => &RELEVANCE_EMIT,
            Self::CapabilityRequest => &CAPABILITY_REQUEST,
        }
    }

    pub const fn result_type(self) -> PythType {
        match self {
            Self::SystemLog | Self::RelevanceEmit => PythType::Unit,
            Self::ObjectCreate | Self::ObjectQuery | Self::GraphRelated => PythType::ObjectId,
            Self::ObjectCreatedCapability | Self::ObjectQueriedCapability => PythType::Capability,
            Self::ObjectCreatedRevision | Self::ObjectInspectedRevision | Self::ObjectRevise => {
                PythType::RevisionId
            }
            Self::ObjectInspect => PythType::Utf8,
            Self::ObjectHistory => PythType::U64,
            Self::TaskActive | Self::TaskContextActive | Self::TaskContextCandidate => {
                PythType::TaskId
            }
            Self::TaskContextScore | Self::TaskContextKind => PythType::U64,
            Self::TaskContextReason => PythType::Utf8,
            Self::TaskPropose => PythType::Unit,
            Self::CapabilityRequest => PythType::ProposalId,
        }
    }

    pub const fn requirement(self) -> Option<CapabilityRequirement> {
        match self {
            Self::SystemLog => Some(CapabilityRequirement {
                resource_kind: RESOURCE_SYSTEM_LOG,
                rights: RIGHTS_READ,
            }),
            Self::ObjectCreate => Some(CapabilityRequirement {
                resource_kind: RESOURCE_OBJECT_WORKSPACE,
                rights: RIGHTS_CREATE,
            }),
            Self::ObjectQuery => Some(CapabilityRequirement {
                resource_kind: RESOURCE_OBJECT_WORKSPACE,
                rights: RIGHTS_QUERY,
            }),
            Self::ObjectInspect | Self::ObjectHistory => Some(CapabilityRequirement {
                resource_kind: RESOURCE_OBJECT,
                rights: RIGHTS_READ,
            }),
            Self::ObjectRevise => Some(CapabilityRequirement {
                resource_kind: RESOURCE_OBJECT,
                rights: RIGHTS_REVISE,
            }),
            Self::TaskActive
            | Self::TaskContextActive
            | Self::TaskContextCandidate
            | Self::TaskContextScore
            | Self::TaskContextKind
            | Self::TaskContextReason => Some(CapabilityRequirement {
                resource_kind: RESOURCE_TASK,
                rights: RIGHTS_READ,
            }),
            Self::TaskPropose => Some(CapabilityRequirement {
                resource_kind: RESOURCE_TASK,
                rights: RIGHTS_CREATE,
            }),
            Self::CapabilityRequest => Some(CapabilityRequirement {
                resource_kind: RESOURCE_TASK,
                rights: RIGHTS_APPEND,
            }),
            Self::GraphRelated => Some(CapabilityRequirement {
                resource_kind: RESOURCE_GRAPH,
                rights: RIGHTS_QUERY,
            }),
            Self::RelevanceEmit => Some(CapabilityRequirement {
                resource_kind: RESOURCE_GRAPH,
                rights: RIGHTS_APPEND,
            }),
            Self::ObjectCreatedCapability
            | Self::ObjectCreatedRevision
            | Self::ObjectQueriedCapability
            | Self::ObjectInspectedRevision => None,
        }
    }

    pub const fn producer(self) -> Option<HostProducer> {
        match self {
            Self::ObjectCreate => Some(HostProducer::ObjectCreate),
            Self::ObjectQuery => Some(HostProducer::ObjectQuery),
            Self::ObjectInspect => Some(HostProducer::ObjectInspect),
            _ => None,
        }
    }

    pub const fn host_result_access(self) -> Option<HostResultAccess> {
        match self {
            Self::ObjectCreatedCapability => Some(HostResultAccess::CreatedCapability),
            Self::ObjectCreatedRevision => Some(HostResultAccess::CreatedRevision),
            Self::ObjectQueriedCapability => Some(HostResultAccess::QueriedCapability),
            Self::ObjectInspectedRevision => Some(HostResultAccess::InspectedRevision),
            _ => None,
        }
    }
}
