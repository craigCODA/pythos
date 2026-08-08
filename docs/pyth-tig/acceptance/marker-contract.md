# PythTIG Acceptance Marker Contract

**Status:** Phase 1 format and Phase 2 runtime markers are active only in their
accepted verification paths. Later-phase markers remain reserved until their
corresponding implementation phase is explicitly authorized and verified.

**Namespace:** `PYTHOS:PYTHTIG:`

Markers are emitted only after the named state transition is true. COM1 is the authoritative ordered stream. A script must reject missing, duplicated where singular, out-of-order, or forbidden markers according to its scenario.

## Format and verifier

```text
PYTHOS:PYTHTIG:PACKAGE_VALID package:<hex> nodes:<decimal> blocks:<decimal>
PYTHOS:PYTHTIG:PACKAGE_REJECTED error:<stable-code>
```

`PACKAGE_VALID` occurs only after shared verification, checksum/canonicalization
validation, and the active runtime-profile check all pass. It occurs before
address-space creation. `PACKAGE_REJECTED` is terminal for that package launch
and must not be followed by runtime-entry markers for the same invocation.

Phase 2 stable rejection codes include `VERIFY_EFFECT_FORK` for shared-verifier
rejection and `UNSUPPORTED_PHASE2_OPCODE` for a well-formed version 1 package
outside the bounded Phase 2 interpreter profile. Both are emitted before
package mapping, bootstrap construction, or ring-3 entry.

## Runtime bootstrap and execution

```text
PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:<hex> imports:<decimal>
PYTHOS:PYTHTIG:RUNTIME_ENTER package:<hex>
PYTHOS:PYTHTIG:PROGRAM_LOG
PYTHOS:PYTHTIG:BUDGET_EXHAUSTED node:<decimal>
PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED principal:<hex>
PYTHOS:PYTHTIG:RUNTIME_EXIT status:<stable-code>
```

Required order for successful interpreted execution:

```text
PACKAGE_VALID
BOOTSTRAP_BOUND
RUNTIME_ENTER
PROGRAM_LOG or other scenario operation markers
RUNTIME_EXIT status:0
```

Budget and fault scenarios must not emit successful completion for the invocation.

## Object and capability flow

```text
PYTHOS:PYTHTIG:OBJECT_CREATED object:<decimal> revision:<decimal>
PYTHOS:PYTHTIG:OBJECT_REVISED object:<decimal> revision:<decimal>
PYTHOS:PYTHTIG:OBJECT_INSPECTED object:<decimal> revision:<decimal>
PYTHOS:PYTHTIG:OBJECT_HISTORY object:<decimal> revisions:<decimal>
PYTHOS:PYTHTIG:OBJECT_KNOWN_DENIED object:<decimal>
PYTHOS:PYTHTIG:CAPABILITY_FORGERY_DENIED
PYTHOS:PYTHTIG:OBJECT_REBOUND object:<decimal>
PYTHOS:PYTHTIG:OBJECT_FLOW_ACCEPTANCE_COMPLETE
```

`CAPABILITY_FORGERY_DENIED` occurs only after holder/resource/rights validation rejects the copied or fabricated authority before object mutation. `OBJECT_REBOUND` proves a fresh runtime capability was minted from stable identity/policy after reboot; it does not claim numeric handle inequality.

## Task environment and Task Steward

```text
PYTHOS:PYTHTIG:TASK_STEWARD_READY
PYTHOS:PYTHTIG:TASK_CONTEXT_STABLE
PYTHOS:PYTHTIG:TASK_DIVERGENCE_DETECTED
PYTHOS:PYTHTIG:TASK_PROPOSAL_CREATED proposal:<decimal> active:<decimal>
PYTHOS:PYTHTIG:TASK_AUTHORITY_UNCHANGED active:<decimal>
PYTHOS:PYTHTIG:TASK_DIRECT_CREATE_DENIED
PYTHOS:PYTHTIG:TASK_PROPOSAL_APPROVED proposal:<decimal>
PYTHOS:PYTHTIG:TASK_CREATED task:<decimal>
PYTHOS:PYTHTIG:TASK_SUSPENDED task:<decimal>
PYTHOS:PYTHTIG:TASK_REVIVED task:<decimal>
PYTHOS:PYTHTIG:TASK_STATE_RESTORED task:<decimal>
PYTHOS:PYTHTIG:TASK_STEWARD_ACCEPTANCE_COMPLETE
```

Stable-context scenario requires `TASK_CONTEXT_STABLE` and forbids `TASK_PROPOSAL_CREATED`.

Divergence scenario requires:

```text
TASK_DIVERGENCE_DETECTED
TASK_PROPOSAL_CREATED
TASK_AUTHORITY_UNCHANGED
TASK_DIRECT_CREATE_DENIED
```

Only the user-authorized path may then emit `TASK_PROPOSAL_APPROVED` and `TASK_CREATED`.

## Native backend and differential evidence

```text
PYTHOS:PYTHTIG:NATIVE_ELF_VALID package:<hex>
PYTHOS:PYTHTIG:NATIVE_ENTER package:<hex>
PYTHOS:PYTHTIG:NATIVE_EXIT status:<stable-code>
PYTHOS:PYTHTIG:DIFFERENTIAL_MATCH case:<stable-name>
```

A differential match requires equal typed exit status, operation result, object revision/history, denial class, and observable marker order for the compared case. It is not a byte-for-byte instruction comparison.

## Cutover, fallback, and cross-target

```text
PYTHOS:PYTHTIG:SESSION_MANAGER_READY
PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY
PYTHOS:PYTHTIG:SERVICE_FAULT_CONTAINED service:<stable-name>
PYTHOS:PYTHTIG:RECOVERY_SHELL_ENTER
PYTHOS:PYTHTIG:ACCEPTANCE_COMPLETE package:<hex> target:<stable-name>
```

`DEFAULT_SERVICES_READY` is emitted only after the Pyth-native session manager and required default services are alive. A service fault scenario requires `SERVICE_FAULT_CONTAINED` followed by `RECOVERY_SHELL_ENTER` and must not panic PythCore.

`ACCEPTANCE_COMPLETE` is the terminal marker for one accepted package/target run. The target name and package digest must be captured in the test artifact.

## Global forbidden interpretations

These markers do not by themselves prove:

- universal hardware support;
- production completeness;
- cryptographic code signing;
- general Python compatibility;
- self-hosting;
- arbitrary third-party application safety;
- an LLM-driven runtime agent;
- broad filesystem, networking, package-management, or update support.
