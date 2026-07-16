//! Kernel-owned service quota proofs for Phase 8.
#![cfg_attr(test, allow(dead_code))]

use crate::service_identity::{ServiceId, ServiceIdentityTable};
use crate::tasks::TaskId;

const MAX_QUOTA_RECORDS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaError {
    TableFull,
    UnknownService,
    MemoryQuotaExceeded,
    CpuQuotaExceeded,
    CounterOverflow,
    IdentityRegistrationFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryQuotaProof {
    pub allocation_granted: bool,
    pub allocation_denied: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuQuotaProof {
    pub tick_recorded: bool,
    pub throttle_denied: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MemoryQuotaRecord {
    service_id: ServiceId,
    max_pages: u64,
    used_pages: u64,
    active: bool,
}

impl MemoryQuotaRecord {
    const fn empty() -> Self {
        Self {
            service_id: ServiceId::invalid(),
            max_pages: 0,
            used_pages: 0,
            active: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MemoryQuotaTable {
    records: [MemoryQuotaRecord; MAX_QUOTA_RECORDS],
}

impl MemoryQuotaTable {
    const fn new() -> Self {
        Self {
            records: [MemoryQuotaRecord::empty(); MAX_QUOTA_RECORDS],
        }
    }

    fn register(&mut self, service_id: ServiceId, max_pages: u64) -> Result<(), QuotaError> {
        if self
            .records
            .iter()
            .any(|record| record.active && record.service_id == service_id)
        {
            return Err(QuotaError::TableFull);
        }
        let record = self
            .records
            .iter_mut()
            .find(|record| !record.active)
            .ok_or(QuotaError::TableFull)?;
        *record = MemoryQuotaRecord {
            service_id,
            max_pages,
            used_pages: 0,
            active: true,
        };
        Ok(())
    }

    fn charge_pages(&mut self, service_id: ServiceId, pages: u64) -> Result<(), QuotaError> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.active && record.service_id == service_id)
            .ok_or(QuotaError::UnknownService)?;
        let next_used = record
            .used_pages
            .checked_add(pages)
            .ok_or(QuotaError::CounterOverflow)?;
        if next_used > record.max_pages {
            return Err(QuotaError::MemoryQuotaExceeded);
        }
        record.used_pages = next_used;
        Ok(())
    }

    fn used_pages(&self, service_id: ServiceId) -> Result<u64, QuotaError> {
        self.records
            .iter()
            .find(|record| record.active && record.service_id == service_id)
            .map(|record| record.used_pages)
            .ok_or(QuotaError::UnknownService)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CpuQuotaRecord {
    service_id: ServiceId,
    max_ticks: u64,
    used_ticks: u64,
    active: bool,
}

impl CpuQuotaRecord {
    const fn empty() -> Self {
        Self {
            service_id: ServiceId::invalid(),
            max_ticks: 0,
            used_ticks: 0,
            active: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CpuQuotaTable {
    records: [CpuQuotaRecord; MAX_QUOTA_RECORDS],
}

impl CpuQuotaTable {
    const fn new() -> Self {
        Self {
            records: [CpuQuotaRecord::empty(); MAX_QUOTA_RECORDS],
        }
    }

    fn register(&mut self, service_id: ServiceId, max_ticks: u64) -> Result<(), QuotaError> {
        if self
            .records
            .iter()
            .any(|record| record.active && record.service_id == service_id)
        {
            return Err(QuotaError::TableFull);
        }
        let record = self
            .records
            .iter_mut()
            .find(|record| !record.active)
            .ok_or(QuotaError::TableFull)?;
        *record = CpuQuotaRecord {
            service_id,
            max_ticks,
            used_ticks: 0,
            active: true,
        };
        Ok(())
    }

    fn charge_tick(&mut self, service_id: ServiceId) -> Result<(), QuotaError> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.active && record.service_id == service_id)
            .ok_or(QuotaError::UnknownService)?;
        let next_used = record
            .used_ticks
            .checked_add(1)
            .ok_or(QuotaError::CounterOverflow)?;
        if next_used > record.max_ticks {
            return Err(QuotaError::CpuQuotaExceeded);
        }
        record.used_ticks = next_used;
        Ok(())
    }

    fn used_ticks(&self, service_id: ServiceId) -> Result<u64, QuotaError> {
        self.records
            .iter()
            .find(|record| record.active && record.service_id == service_id)
            .map(|record| record.used_ticks)
            .ok_or(QuotaError::UnknownService)
    }
}

pub fn run_memory_self_test() -> Result<MemoryQuotaProof, QuotaError> {
    let mut identities = ServiceIdentityTable::new();
    let service_id = identities
        .register_task(TaskId::new(85))
        .map_err(|_| QuotaError::IdentityRegistrationFailed)?;

    let mut quotas = MemoryQuotaTable::new();
    quotas.register(service_id, 2)?;
    quotas.charge_pages(service_id, 1)?;
    let used_after_grant = quotas.used_pages(service_id)?;

    let denied = quotas.charge_pages(service_id, 2) == Err(QuotaError::MemoryQuotaExceeded);
    if quotas.used_pages(service_id)? != used_after_grant {
        return Err(QuotaError::MemoryQuotaExceeded);
    }

    Ok(MemoryQuotaProof {
        allocation_granted: used_after_grant == 1,
        allocation_denied: denied,
    })
}

pub fn run_cpu_self_test() -> Result<CpuQuotaProof, QuotaError> {
    let mut identities = ServiceIdentityTable::new();
    let service_id = identities
        .register_task(TaskId::new(87))
        .map_err(|_| QuotaError::IdentityRegistrationFailed)?;

    let mut quotas = CpuQuotaTable::new();
    quotas.register(service_id, 2)?;
    quotas.charge_tick(service_id)?;
    let used_after_first_tick = quotas.used_ticks(service_id)?;
    quotas.charge_tick(service_id)?;
    let used_at_budget = quotas.used_ticks(service_id)?;

    let throttled = quotas.charge_tick(service_id) == Err(QuotaError::CpuQuotaExceeded);
    if quotas.used_ticks(service_id)? != used_at_budget {
        return Err(QuotaError::CpuQuotaExceeded);
    }

    Ok(CpuQuotaProof {
        tick_recorded: used_after_first_tick == 1,
        throttle_denied: throttled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_quota_denies_over_limit_without_mutating_usage() {
        let mut identities = ServiceIdentityTable::new();
        let service_id = identities.register_task(TaskId::new(85)).unwrap();
        let mut quotas = MemoryQuotaTable::new();

        quotas.register(service_id, 2).unwrap();
        quotas.charge_pages(service_id, 1).unwrap();

        assert_eq!(
            quotas.charge_pages(service_id, 2),
            Err(QuotaError::MemoryQuotaExceeded)
        );
        assert_eq!(quotas.used_pages(service_id), Ok(1));
    }

    #[test]
    fn memory_quota_requires_registered_service() {
        let mut identities = ServiceIdentityTable::new();
        let unknown_service = identities.register_task(TaskId::new(86)).unwrap();
        let mut quotas = MemoryQuotaTable::new();

        assert_eq!(
            quotas.charge_pages(unknown_service, 1),
            Err(QuotaError::UnknownService)
        );
    }

    #[test]
    fn memory_quota_self_test_proves_grant_and_denial() {
        let proof = run_memory_self_test().unwrap();

        assert!(proof.allocation_granted);
        assert!(proof.allocation_denied);
    }

    #[test]
    fn cpu_quota_records_in_budget_tick() {
        let mut identities = ServiceIdentityTable::new();
        let service_id = identities.register_task(TaskId::new(87)).unwrap();
        let mut quotas = CpuQuotaTable::new();

        quotas.register(service_id, 2).unwrap();
        quotas.charge_tick(service_id).unwrap();

        assert_eq!(quotas.used_ticks(service_id), Ok(1));
    }

    #[test]
    fn cpu_quota_throttles_over_budget_without_mutating_usage() {
        let mut identities = ServiceIdentityTable::new();
        let service_id = identities.register_task(TaskId::new(88)).unwrap();
        let mut quotas = CpuQuotaTable::new();

        quotas.register(service_id, 2).unwrap();
        quotas.charge_tick(service_id).unwrap();
        quotas.charge_tick(service_id).unwrap();

        assert_eq!(
            quotas.charge_tick(service_id),
            Err(QuotaError::CpuQuotaExceeded)
        );
        assert_eq!(quotas.used_ticks(service_id), Ok(2));
    }

    #[test]
    fn cpu_quota_self_test_proves_tick_and_throttle() {
        let proof = run_cpu_self_test().unwrap();

        assert!(proof.tick_recorded);
        assert!(proof.throttle_denied);
    }
}
