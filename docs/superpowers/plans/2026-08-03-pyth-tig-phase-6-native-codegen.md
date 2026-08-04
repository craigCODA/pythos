# PythTIG Phase 6 Native x86-64 Code Generation Implementation Plan

**Status:** Proposed future phase pending owner adoption of ADR 0064 and ADR 0065. Do not implement this plan until Phase 0 is reviewed and the owner explicitly invokes this phase.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. The interpreter is the semantic oracle. Every native case requires differential evidence.

**Goal:** Lower verified PythTIG packages into custom-generated x86-64 ring-3 ELF executables and prove semantic equivalence with the interpreter for success, control flow, budget exhaustion, object operations, capability denial, and faults.

**Architecture:** A host-side custom emitter accepts only `VerifiedGraph`. It assigns one fixed stack slot per graph value, emits direct x86-64 instructions for pure/control nodes, emits custom syscall stubs for host operations, decrements a native instruction budget before each node, and writes an ET_EXEC ELF without LLVM or a conventional language runtime.

**Tech Stack:** Rust `std` custom machine-code and ELF writer, shared verifier, existing user ELF loader and syscall ABI, QEMU differential harness.

## Global Constraints

- No lowering from unverified bytes.
- No LLVM, Cranelift, GCC, Clang, or external assembler in the accepted backend.
- Generated programs have no writable-executable segment.
- Graph values never expose codegen stack addresses.
- Capability constants remain impossible.
- Native execution must produce the same typed results and object-service effects as the interpreter.
- The interpreter remains the reference implementation after native acceptance.

---

### Task 1: Create custom x86-64 encoder and ELF writer

**Files:**
- Modify: `Cargo.toml`
- Create: `tools/pyth-codegen-x86_64/Cargo.toml`
- Create: `tools/pyth-codegen-x86_64/src/lib.rs`
- Create: `tools/pyth-codegen-x86_64/src/x86.rs`
- Create: `tools/pyth-codegen-x86_64/src/elf.rs`
- Create: `tools/pyth-codegen-x86_64/src/patch.rs`
- Create: `tools/pyth-codegen-x86_64/tests/encoder.rs`

**Interfaces:**
- Produces: bounded x86 instruction encoder, label patcher, ET_EXEC writer.

- [ ] **Step 1: Write failing encoder tests**

```rust
#[test]
fn encodes_required_integer_branch_and_syscall_instructions() {
    let mut code = CodeBuffer::new();
    code.mov_imm64(Register::Rax, 0x1122_3344_5566_7788).unwrap();
    code.cmp_reg64(Register::Rax, Register::Rbx).unwrap();
    code.jz(Label::new(1)).unwrap();
    code.syscall().unwrap();
    assert_eq!(&code.bytes()[..10], &[0x48, 0xB8, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11]);
    assert!(code.has_unresolved_label(Label::new(1)));
}

#[test]
fn elf_writer_produces_exec_with_rx_text_and_rw_data() {
    let elf = ElfImage::new(0x0040_0000)
        .with_text(&[0xC3])
        .with_rodata(b"hello")
        .with_data(&[0u8; 32])
        .encode()
        .unwrap();
    let parsed = test_support::parse_elf(&elf).unwrap();
    assert_eq!(parsed.file_type, ET_EXEC);
    assert!(parsed.has_rx_load);
    assert!(parsed.has_rw_load);
    assert!(!parsed.has_rwx_load);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pyth-codegen-x86_64 --test encoder
```

- [ ] **Step 3: Implement only required instructions**

Support:

```text
push/pop callee-saved registers
mov imm64/reg64/mem64
lea
add/sub
and/or/xor
cmp/test
setcc
movzx
call relative
jmp/jcc relative
syscall
ud2
ret
```

Every encoder method checks buffer capacity and operand restrictions. Label patching uses signed 32-bit relative displacements and rejects overflow.

- [ ] **Step 4: Implement ELF writer**

Generate ELF64 little-endian x86-64 ET_EXEC with three aligned LOAD segments:

```text
.text   RX at 0x00400000
.rodata R  at next 0x1000 boundary
.data   RW at next 0x1000 boundary
```

No section headers are required for execution, but include `.text`, `.rodata`, `.data`, and `.shstrtab` for inspection. Entry is `_start` in `.text`.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test -p pyth-codegen-x86_64 --test encoder
```

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml tools\pyth-codegen-x86_64
git commit -m "feat(pyth-native): add custom x86 and ELF encoder"
```

---

### Task 2: Lower pure values, control flow, and native budget

**Files:**
- Create: `tools/pyth-codegen-x86_64/src/layout.rs`
- Create: `tools/pyth-codegen-x86_64/src/lower.rs`
- Create: `tools/pyth-codegen-x86_64/src/runtime_layout.rs`
- Create: `tools/pyth-codegen-x86_64/tests/lower.rs`

**Interfaces:**
- Produces: `lower_verified_graph(VerifiedGraph<'_>) -> Result<NativeImage, CodegenError>`.

- [ ] **Step 1: Write failing lowering tests**

```rust
#[test]
fn assigns_fixed_slots_and_emits_budget_check_per_node() {
    let graph = verified_fixture("branch-log.tig");
    let plan = NativeLayout::plan(&graph).unwrap();
    assert_eq!(plan.value_slot_count(), graph.package().nodes().len());
    let image = lower_verified_graph(graph).unwrap();
    assert_eq!(image.metadata.budget_checks, image.metadata.executable_nodes);
    assert!(image.metadata.branch_patches > 0);
}

#[test]
fn rejects_graph_larger_than_native_stack_budget() {
    let graph = test_support::verified_graph_with_nodes(1024);
    assert_eq!(
        NativeLayout::plan(&graph),
        Err(CodegenError::StackFrameTooLarge { required: 16_384, maximum: 12_288 })
    );
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pyth-codegen-x86_64 --test lower
```

- [ ] **Step 3: Implement native frame contract**

At `_start`, `rdi` contains `PythGraphBootstrapBlock*`. Generated code:

1. validates bootstrap magic and ABI;
2. saves `r12-r15`;
3. stores bootstrap pointer in `r12`;
4. allocates a 16-byte-aligned stack frame;
5. stores instruction budget in a dedicated slot;
6. zeroes `GraphExitRecord` in `.data` or bootstrap result mapping;
7. enters the entry block.

Each graph value gets 16 bytes:

```text
bytes 0..7  payload
bytes 8..9  PythType code
bytes 10..15 reserved zero
```

Before every executable node:

```text
if budget == 0 -> budget-exhausted exit
budget -= 1
```

Implement pure and control opcodes from ADR 0065, including block-parameter moves on control edges.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test -p pyth-codegen-x86_64 --test lower
```

- [ ] **Step 5: Commit**

```powershell
git add tools\pyth-codegen-x86_64\src\layout.rs tools\pyth-codegen-x86_64\src\lower.rs tools\pyth-codegen-x86_64\src\runtime_layout.rs tools\pyth-codegen-x86_64\tests\lower.rs
git commit -m "feat(pyth-native): lower graph control and values"
```

---

### Task 3: Emit capability imports and typed syscall stubs

**Files:**
- Create: `tools/pyth-codegen-x86_64/src/stubs.rs`
- Modify: `tools/pyth-codegen-x86_64/src/lower.rs`
- Modify: `tools/pyth-codegen-x86_64/tests/lower.rs`

**Interfaces:**
- Produces: native SystemLog, object, task, and graph-exit stubs using existing syscall numbers.

- [ ] **Step 1: Write failing stub tests**

```rust
#[test]
fn host_operations_load_capabilities_from_bootstrap_not_immediates() {
    let graph = verified_fixture("object-create.tig");
    let image = lower_verified_graph(graph).unwrap();
    assert_eq!(image.metadata.capability_immediates, 0);
    assert!(image.metadata.bootstrap_import_loads >= 2);
    assert!(image.metadata.object_syscalls >= 3);
}

#[test]
fn generated_request_buffers_are_writable_non_executable_data() {
    let graph = verified_fixture("object-create.tig");
    let image = lower_verified_graph(graph).unwrap();
    let parsed = test_support::parse_elf(&image.bytes).unwrap();
    assert!(parsed.symbol_in_rw_segment("PYTH_OBJECT_REQUEST"));
    assert!(!parsed.symbol_in_rx_segment("PYTH_OBJECT_REQUEST"));
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pyth-codegen-x86_64 host_operations
```

- [ ] **Step 3: Implement syscall stubs**

Generate fixed routines for:

```text
system_log
object_request
task_request
graph_exit
```

Stubs use the existing x86-64 syscall register ABI. Request and response buffers live in the RW segment. Capability values are loaded from the bootstrap import table or dynamic host-result value slots. The emitter never embeds a runtime capability handle in text or rodata.

- [ ] **Step 4: Lower effect and HostResult nodes**

Effect nodes call the corresponding stub and store a `HostCallResult`. `HostResult` copies the selected typed field into its value slot. Denied status remains a typed result and does not crash the program.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test -p pyth-codegen-x86_64
```

- [ ] **Step 6: Commit**

```powershell
git add tools\pyth-codegen-x86_64\src\stubs.rs tools\pyth-codegen-x86_64\src\lower.rs tools\pyth-codegen-x86_64\tests\lower.rs
git commit -m "feat(pyth-native): emit capability-gated syscall stubs"
```

---

### Task 4: Add codegen CLI and ELF verification

**Files:**
- Create: `tools/pyth-codegen-x86_64/src/main.rs`
- Create: `scripts/build-pyth-native.py`
- Create: `scripts/verify-pyth-native-elf.py`
- Modify: `scripts/build-image.py`
- Modify: `scripts/build-iso.py`

**Interfaces:**
- Produces: `pyth-codegen-x86_64 build <tig> -o <elf>` and test image packaging.

- [ ] **Step 1: Write failing CLI test**

`verify-pyth-native-elf.py` requires:

```text
ET_EXEC
x86-64
entry in RX LOAD
no RWE LOAD
RW request/result region
no dynamic section
no interpreter segment
```

- [ ] **Step 2: Run RED**

```powershell
python scripts\build-pyth-native.py programs\examples\hello.pyth
python scripts\verify-pyth-native-elf.py target\pyth-native\hello.elf
```

- [ ] **Step 3: Implement CLI**

```text
pyth-codegen-x86_64 build <package.tig> -o <program.elf>
pyth-codegen-x86_64 inspect <program.elf>
```

The `build` command decodes and verifies before lowering. Verifier failure exits 3 and writes no output file.

- [ ] **Step 4: Package native programs**

`build-image.py --pyth-native-elf <path>` packages the generated ELF as a named user program using the graph package principal. A separate manifest record binds the ELF digest to the source graph package digest for differential evidence; this is integrity metadata, not a signature claim.

- [ ] **Step 5: Run GREEN**

```powershell
python scripts\build-pyth-native.py programs\examples\hello.pyth
python scripts\verify-pyth-native-elf.py target\pyth-native\hello.elf
```

Expected: `PYTH_NATIVE_ELF_VERIFY_OK`.

- [ ] **Step 6: Commit**

```powershell
git add tools\pyth-codegen-x86_64\src\main.rs scripts\build-pyth-native.py scripts\verify-pyth-native-elf.py scripts\build-image.py scripts\build-iso.py
git commit -m "feat(pyth-native): build standalone graph executables"
```

---

### Task 5: Differential interpreter/native acceptance

**Files:**
- Create: `scripts/test-pyth-native-codegen.py`
- Modify: `core/src/normal_boot.rs`
- Modify: `core/src/runtime_loader.rs`
- Modify: `.github/workflows/qemu-acceptance.yml`

**Interfaces:**
- Produces: `PYTH_NATIVE_CODEGEN_TEST_OK`.

- [ ] **Step 1: Write differential harness**

For each program below, run interpreter and native modes against clean or shared storage as appropriate:

```text
hello.pyth
branch-log.pyth
budget-loop.pyth
object-create.pyth
object-restore.pyth
object-known-denied.pyth
task-steward/main.pyth
```

Capture a normalized typed result record:

```json
{
  "status": 0,
  "error_code": 0,
  "executed_nodes": 17,
  "object_ids": [1042],
  "revisions": [2],
  "denials": ["known-object"],
  "task_proposals": [3001]
}
```

Ignore implementation-specific instruction addresses and ELF sizes. Require exact equality for status, error code, object/task effects, denial categories, and program log text. For executed count, compare graph nodes executed, not machine instructions.

- [ ] **Step 2: Run RED**

```powershell
python scripts\test-pyth-native-codegen.py
```

- [ ] **Step 3: Add native test-control mode**

Control sector mode 3 launches the selected native ELF. PythCore still derives caller identity from the named user manifest, binds capabilities by principal policy, and validates every syscall.

Required markers:

```text
PYTHOS:PYTHTIG:NATIVE_ELF_VALID
PYTHOS:PYTHTIG:NATIVE_ENTER
PYTHOS:PYTHTIG:NATIVE_EXIT
PYTHOS:PYTHTIG:DIFFERENTIAL_MATCH
```

- [ ] **Step 4: Add fault and forgery differentials**

Include:

```text
budget exhaustion
wrong-holder copied capability
known-object denial
invalid syscall pointer
intentional ud2 user fault
```

Both modes must classify the outcome identically and leave PythCore/peer alive.

- [ ] **Step 5: Run GREEN and preserved suites**

```powershell
python scripts\test-pyth-native-codegen.py
python scripts\test-pyth-task-steward.py
python scripts\test-pyth-graph-object-flow.py
python scripts\test-object-shell.py
python scripts\test-boot.py
```

Expected all success markers.

- [ ] **Step 6: Add CI and commit**

```yaml
- name: Pyth native differential acceptance
  run: python scripts/test-pyth-native-codegen.py
```

```powershell
git add scripts\test-pyth-native-codegen.py core\src\normal_boot.rs core\src\runtime_loader.rs .github\workflows\qemu-acceptance.yml
git commit -m "test(pyth-native): prove interpreter native equivalence"
```

---

## Phase 6 verification

```powershell
cargo fmt --all -- --check
cargo test -p pyth-codegen-x86_64
cargo clippy -p pyth-codegen-x86_64 --all-targets -- -D warnings
python scripts\verify-pyth-native-elf.py target\pyth-native\hello.elf
python scripts\test-pyth-native-codegen.py
python scripts\test-pyth-task-steward.py
python scripts\test-pyth-graph-object-flow.py
python scripts\test-object-shell.py
python scripts\test-boot.py
```

Dispatch the runtime reviewer, security reviewer, and final codegen specialist. Any semantic mismatch, embedded capability, RWE segment, or verifier bypass blocks Phase 7.
