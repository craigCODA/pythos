//! Shell-side per-object capability cache (ADR 0051).
//!
//! Object IDs identify objects; they do not authorize access. This map is the
//! shell's own bookkeeping of which capability it currently holds for which
//! object id — never a substitute for holding the real capability. If an
//! object id has no cached entry, the caller must re-query the workspace
//! before it can act on that object; it must never fall back to using the
//! workspace capability for a per-object operation.

use pythos_shared::object_shell_abi::{
    BootstrapCapabilityBlock, ObjectListEntry, PackedCapability,
};

const CAPACITY: usize = pythos_shared::object_shell_abi::MAX_SHELL_OBJECT_CAPS;

#[derive(Clone, Copy)]
pub struct CapabilityMap {
    console: PackedCapability,
    workspace: PackedCapability,
    system_control: PackedCapability,
    entries: [Option<ObjectListEntry>; CAPACITY],
    entry_count: usize,
}

impl CapabilityMap {
    /// Seed the map from the read-only bootstrap block PythCore mapped into
    /// this process at launch.
    pub fn from_bootstrap(bootstrap: &BootstrapCapabilityBlock) -> Self {
        let mut map = Self {
            console: bootstrap.console,
            workspace: bootstrap.workspace,
            system_control: bootstrap.system_control,
            entries: [None; CAPACITY],
            entry_count: 0,
        };
        let object_count = usize::from(bootstrap.object_count).min(CAPACITY);
        for entry in bootstrap.objects.iter().take(object_count) {
            map.remember(entry.object_id, entry.capability);
        }
        map
    }

    pub const fn console(&self) -> PackedCapability {
        self.console
    }

    pub const fn workspace(&self) -> PackedCapability {
        self.workspace
    }

    pub const fn system_control(&self) -> PackedCapability {
        self.system_control
    }

    /// The cached capability for `object_id`, if any. `None` means the shell
    /// must re-query the workspace before it can act on this object.
    pub fn object_capability(&self, object_id: u64) -> Option<PackedCapability> {
        self.entries[..self.entry_count]
            .iter()
            .flatten()
            .find(|entry| entry.object_id == object_id)
            .map(|entry| entry.capability)
    }

    /// Record (or update) the capability for one object id, e.g. the
    /// capability `create` returns, or one entry from a `query` response.
    /// Bounded to `MAX_SHELL_OBJECT_CAPS`; the oldest entry is evicted to
    /// make room for a new one once full.
    pub fn remember(&mut self, object_id: u64, capability: PackedCapability) {
        if let Some(existing) = self.entries[..self.entry_count]
            .iter_mut()
            .flatten()
            .find(|entry| entry.object_id == object_id)
        {
            existing.capability = capability;
            return;
        }
        let slot = if self.entry_count < CAPACITY {
            let index = self.entry_count;
            self.entry_count += 1;
            index
        } else {
            0 // evict the oldest (first) entry once full
        };
        if slot == 0 && self.entry_count == CAPACITY {
            self.entries.copy_within(1.., 0);
            self.entries[CAPACITY - 1] = None;
            self.entries[CAPACITY - 1] = Some(ObjectListEntry {
                object_id,
                capability,
            });
        } else {
            self.entries[slot] = Some(ObjectListEntry {
                object_id,
                capability,
            });
        }
    }

    /// Absorb a `query` response's entries in one pass.
    pub fn remember_query_results(&mut self, results: &[ObjectListEntry]) {
        for entry in results {
            self.remember(entry.object_id, entry.capability);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bootstrap_with(objects: &[(u64, u64)]) -> BootstrapCapabilityBlock {
        let mut block = BootstrapCapabilityBlock {
            magic: pythos_shared::object_shell_abi::SHELL_BOOTSTRAP_MAGIC,
            abi_major: 1,
            abi_minor: 0,
            object_count: objects.len() as u16,
            reserved0: 0,
            console: PackedCapability::from_parts(1, 1),
            workspace: PackedCapability::from_parts(2, 1),
            system_control: PackedCapability::from_parts(3, 1),
            objects: [ObjectListEntry {
                object_id: 0,
                capability: PackedCapability::from_raw(0),
            }; CAPACITY],
        };
        for (index, (object_id, cap_slot)) in objects.iter().enumerate() {
            block.objects[index] = ObjectListEntry {
                object_id: *object_id,
                capability: PackedCapability::from_parts(*cap_slot as u32, 1),
            };
        }
        block
    }

    #[test]
    fn seeds_workspace_and_object_entries_from_bootstrap() {
        let bootstrap = bootstrap_with(&[(1042, 10)]);
        let map = CapabilityMap::from_bootstrap(&bootstrap);

        assert_eq!(map.workspace().slot(), 2);
        assert_eq!(map.object_capability(1042).unwrap().slot(), 10);
        assert_eq!(map.object_capability(9999), None);
    }

    #[test]
    fn unknown_object_id_requires_requery_not_workspace_fallback() {
        let bootstrap = bootstrap_with(&[]);
        let map = CapabilityMap::from_bootstrap(&bootstrap);

        assert_eq!(map.object_capability(1), None);
        // The workspace capability must never be substituted here by the
        // caller; this map simply has nothing cached for object 1.
        assert_ne!(map.workspace().raw(), 0);
    }

    #[test]
    fn remember_updates_existing_entry_in_place() {
        let bootstrap = bootstrap_with(&[(1042, 10)]);
        let mut map = CapabilityMap::from_bootstrap(&bootstrap);

        map.remember(1042, PackedCapability::from_parts(10, 2));

        assert_eq!(map.object_capability(1042).unwrap().generation(), 2);
    }

    #[test]
    fn remember_evicts_oldest_entry_once_full() {
        let mut map = CapabilityMap::from_bootstrap(&bootstrap_with(&[]));
        for id in 0..CAPACITY as u64 {
            map.remember(id, PackedCapability::from_parts(id as u32, 1));
        }
        // Map is now full (0..CAPACITY). Adding one more evicts object 0.
        map.remember(CAPACITY as u64, PackedCapability::from_parts(99, 1));

        assert_eq!(map.object_capability(0), None);
        assert_eq!(map.object_capability(CAPACITY as u64).unwrap().slot(), 99);
    }
}
