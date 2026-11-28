# ADR 0121 — Kai's native interaction model

Status: Owner direction recorded 2026-11-27. This governs design and
reassessment of the Kai workstream. Implementation scope comes from each
separately invoked slice.

## Decision

PythOS is an anti-desktop operating environment. A person enters a world they
can author. Actions in that world address typed objects, tasks, and explicit
relationships. The vault holds their identities, revisions, executable
procedures, continuations, and evidence. PythCore enforces the capabilities
that authorize effects. Seeing or naming something never grants authority.

Reject POSIX and Unix lineage at the design level, including in the relational
model. Object identity, naming, traversal, relationships, execution, service
lifetime, and authority must come from PythOS's own requirements. A graph
schema must not reproduce an inherited hierarchy, shell session, descriptor
model, ambient namespace, or desktop with new names. If a proposed or active
slice starts depending on such a model, stop that slice, record the dependency,
and reassess the operation using native objects, tasks, and capabilities.

This applies [ADR 0064](0064-pyth-native-typed-instruction-graph.md) and
[ADR 0066](0066-supersede-desktop-shell-authority.md) to Kai. Hardware and
firmware interfaces such as UEFI and NVMe provide mechanisms. Host tools and
test harnesses provide evidence. The authored world projects the native object
and task model.

## Reassessed work

| Work | Evidence and standing |
| --- | --- |
| Hosted terminal experiment | Closed as a path to native Kai. The Windows account and shell owned execution. The experiment provides historical host-side evidence. |
| Hosted investigation experiment | Carry forward the goal, evidence, prediction, and outcome links. New Kai interactions start with typed object and revision references and explicit relationships. |
| [Editable world](0114-native-editable-world.md) and [graph presentation](0118-native-graph-window.md) | Retain the evidence for authored objects, committed bindings, native input, rendering, and recovery. Renderer focus and placement carry presentation state. A visible surface projects the relevant objects or tasks. |
| [NVMe boot root](0120-kai-vault-boot-root.md) | Retain the bounded proof that disk-resident task, program, and continuation determine work after PythCore takes over. The kernel admits the storage and commits the result. The evaluator runs in a protected PythOS task. A resident Kai session and a firmware-stage graph remain future work. |

The [earlier locator contract](0070-phase-12-object-locator-resolution-abi.md)
preserves its scoped records and evidence. Its slash spelling does not extend
into the new Kai interaction design. Finite transport experiments preserve
their evidence and scope. Define new operations through PythOS-native
relationships and capabilities.

## Candidate for the next design

Select one authored world object that refers to a saved Kai task. Resolve
the specific task and revision through that relationship. With the appropriate
capability, perform one bounded action and commit the resulting continuation
and receipt. The projection exposes the actual outcome and its support
records. Check the same identity and state after restart and show an explicit
denial when the caller lacks the action capability.

This is a design candidate. The current boot proof supplies recovery evidence.
The proposed interaction needs its own contract and proof.
