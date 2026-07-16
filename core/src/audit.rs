//! Fixed capability audit event stream for Phase 3.
#![cfg_attr(test, allow(dead_code))]

use crate::capabilities::ResourceId;
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityTable};
use crate::tasks::TaskId;

const AUDIT_EVENTS: usize = 8;
const AUDIT_RESOURCE: ResourceId = ResourceId::new(0xA017_0001);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditError {
    LogFull,
    MissingEvent,
    InvalidService,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditOperation {
    Grant,
    Use,
    Denial,
    Revocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditOutcome {
    Allowed,
    Denied,
    Revoked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuditEvent {
    service: ServiceId,
    resource: ResourceId,
    operation: AuditOperation,
    outcome: AuditOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuditLog {
    events: [Option<AuditEvent>; AUDIT_EVENTS],
    len: usize,
}

impl AuditLog {
    pub const fn new() -> Self {
        Self {
            events: [None; AUDIT_EVENTS],
            len: 0,
        }
    }

    pub fn record(&mut self, event: AuditEvent) -> Result<(), AuditError> {
        if self.len == self.events.len() {
            return Err(AuditError::LogFull);
        }
        self.events[self.len] = Some(event);
        self.len += 1;
        Ok(())
    }

    pub fn event(&self, index: usize) -> Option<AuditEvent> {
        self.events.get(index).and_then(|event| *event)
    }
}

pub fn run_audit_logging_self_test() -> Result<(), AuditError> {
    let mut identities = ServiceIdentityTable::new();
    let service = identities
        .register_task(TaskId::new(60))
        .map_err(|_| AuditError::InvalidService)?;
    let mut log = AuditLog::new();

    log.record(AuditEvent {
        service,
        resource: AUDIT_RESOURCE,
        operation: AuditOperation::Grant,
        outcome: AuditOutcome::Allowed,
    })?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:AUDIT:GRANT");

    log.record(AuditEvent {
        service,
        resource: AUDIT_RESOURCE,
        operation: AuditOperation::Use,
        outcome: AuditOutcome::Allowed,
    })?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:AUDIT:USE");

    log.record(AuditEvent {
        service,
        resource: AUDIT_RESOURCE,
        operation: AuditOperation::Denial,
        outcome: AuditOutcome::Denied,
    })?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:AUDIT:DENIAL");

    log.record(AuditEvent {
        service,
        resource: AUDIT_RESOURCE,
        operation: AuditOperation::Revocation,
        outcome: AuditOutcome::Revoked,
    })?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:AUDIT:REVOCATION");

    if log.event(0).map(|event| event.operation) != Some(AuditOperation::Grant)
        || log.event(1).map(|event| event.operation) != Some(AuditOperation::Use)
        || log.event(2).map(|event| event.outcome) != Some(AuditOutcome::Denied)
        || log.event(3).map(|event| event.outcome) != Some(AuditOutcome::Revoked)
    {
        return Err(AuditError::MissingEvent);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_log_records_security_relevant_events_in_order() {
        let mut identities = ServiceIdentityTable::new();
        let service = identities.register_task(TaskId::new(60)).unwrap();
        let mut log = AuditLog::new();
        let events = [
            AuditEvent {
                service,
                resource: AUDIT_RESOURCE,
                operation: AuditOperation::Grant,
                outcome: AuditOutcome::Allowed,
            },
            AuditEvent {
                service,
                resource: AUDIT_RESOURCE,
                operation: AuditOperation::Use,
                outcome: AuditOutcome::Allowed,
            },
            AuditEvent {
                service,
                resource: AUDIT_RESOURCE,
                operation: AuditOperation::Denial,
                outcome: AuditOutcome::Denied,
            },
            AuditEvent {
                service,
                resource: AUDIT_RESOURCE,
                operation: AuditOperation::Revocation,
                outcome: AuditOutcome::Revoked,
            },
        ];

        for event in events {
            assert_eq!(log.record(event), Ok(()));
        }

        assert_eq!(log.event(0), Some(events[0]));
        assert_eq!(log.event(1), Some(events[1]));
        assert_eq!(log.event(2), Some(events[2]));
        assert_eq!(log.event(3), Some(events[3]));
    }
}
