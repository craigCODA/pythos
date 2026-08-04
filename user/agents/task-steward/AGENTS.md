# Task Steward Runtime Agent Contract

**Destination:** `user/agents/task-steward/AGENTS.md`

**Status:** Proposed guardrail pending owner adoption of ADR 0064 and ADR 0065.
This file does not authorize implementation before a phase-specific
re-invocation.

## Identity

Task Steward is a deterministic PythTIG runtime program. It is not a Codex/Claude development subagent and it is not a language model.

## Allowed behavior

Task Steward may:

- read only task context, task events, object relationships, and tool descriptors for which it holds capabilities;
- calculate explicit relevance scores from typed inputs;
- propose continuation, new task, subtask, branch, related-task, or revival relationships;
- emit `TaskProposal`, `RelevanceAssertion`, and `CapabilityRequest` objects;
- explain every proposal through source event IDs, weights, and reason data.

## Forbidden behavior

Task Steward may not:

- establish an active task;
- approve its own proposal;
- suspend, revive, merge, complete, or abandon a task;
- mutate authoritative user objects merely because they are relevant;
- treat previous access, ownership, confidence, or semantic similarity as authority;
- inspect an object without an explicit capability;
- fabricate a capability, task ID, proposal approval, or user action;
- hide an uncertainty or unsupported inference.

## Implementation rules

1. Version 1 uses deterministic graph logic, explicit scores, and typed host operations. No LLM dependency.
2. Stable context produces no proposal.
3. Divergent context may produce a proposal, but the active task remains unchanged.
4. Proposal evidence is persisted and replayable across reboot.
5. Relevance scoring is derived state and may be rebuilt. Task identity, status, approval, and history are authoritative typed objects.
6. A wrong-holder or missing capability must produce a denial without a fallback to broader workspace authority.
7. Every acceptance scenario includes a negative proof that direct task creation is denied.

## Mandatory local checks

```powershell
cargo test -p pythos-task-service
python scripts\test-pyth-task-steward.py
```
