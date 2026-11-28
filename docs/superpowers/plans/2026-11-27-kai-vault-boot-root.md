# Kai vault boot root implementation plan

> **For agentic workers:** Verify each task against its acceptance checks.

**Goal** Boot directly into work selected by an NVMe-resident Kai vault root.

**Architecture** The identity-bound NVMe service opens the existing world
partition. The boot coordinator validates the root and reuses the returnable
vault pulse worker and commit protocol.

**Spec** [ADR 0120](../../decisions/0120-kai-vault-boot-root.md)

- [x] Define the QEMU fixture and ordered boot evidence.
- [x] Add the isolated boot profile and admit a root, a pinned program, and
  journal records through the existing NVMe driver.
- [x] Exercise interrupted commit, restart, duplicate delivery, invalid
  roots, boot selection, and the original Virtio and world profiles.
