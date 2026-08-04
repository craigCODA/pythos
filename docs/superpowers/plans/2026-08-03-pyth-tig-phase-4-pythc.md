# PythTIG Phase 4 Pyth Compiler Implementation Plan

**Status:** Accepted future phase pending prior PythTIG phase evidence and
explicit owner invocation. Do not implement this plan until the owner explicitly
invokes this phase.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. The compiler is test-first and may not bypass the shared verifier.

**Goal:** Build the first custom Pyth source compiler that parses a bounded language, type-checks it, lowers it into canonical PythTIG, and reproduces the accepted hello and object-flow programs.

**Architecture:** `pythc` is a host-side Rust tool. Source syntax is intentionally small: one program, declared capability imports, one `main` function, local bindings, conditionals, budgeted loops, intrinsic calls, and return. The shared verifier is the final semantic authority.

**Tech Stack:** Rust `std` compiler crate, shared `no_std` graph ABI/verifier, golden source and binary fixtures, existing QEMU graph acceptance.

## Global Constraints

- PythCore never parses source.
- The compiler emits only canonical packages accepted by `pythos-shared` verifier.
- No user-defined functions, recursion, macros, classes, modules, package manager, or dynamic imports in version 1.
- Every loop has a literal positive `budget` value.
- Capabilities are declared imports and cannot be constructed, cast, compared numerically, or printed as integers.
- Compiler diagnostics contain stable codes and source spans.

---

### Task 1: Create lexer, tokens, parser, and AST

**Files:**
- Modify: `Cargo.toml`
- Create: `tools/pythc/Cargo.toml`
- Create: `tools/pythc/src/main.rs`
- Create: `tools/pythc/src/lib.rs`
- Create: `tools/pythc/src/span.rs`
- Create: `tools/pythc/src/token.rs`
- Create: `tools/pythc/src/lexer.rs`
- Create: `tools/pythc/src/ast.rs`
- Create: `tools/pythc/src/parser.rs`
- Create: `tools/pythc/tests/parser.rs`

**Interfaces:**
- Produces: `lex(source: &str) -> Result<Vec<Token>, Diagnostic>`, `parse_program(tokens: &[Token]) -> Result<Program, Diagnostic>`.

- [ ] **Step 1: Write failing parser tests**

```rust
#[test]
fn parses_minimal_program_and_capability_import() {
    let source = r#"
program hello principal 0x5059544847520001 {
    import log: capability<system.log, write>;
    fn main() -> unit {
        system.log(log, "hello");
        return;
    }
}
"#;
    let program = parse_source(source).unwrap();
    assert_eq!(program.name.text, "hello");
    assert_eq!(program.principal_id, 0x5059_5448_4752_0001);
    assert_eq!(program.imports.len(), 1);
    assert_eq!(program.main.statements.len(), 2);
}

#[test]
fn rejects_unbudgeted_while_and_second_function() {
    let unbudgeted = "program x principal 0x1 { fn main() -> unit { while true { return; } } }";
    assert_eq!(parse_source(unbudgeted).unwrap_err().code, "P0007");

    let second = "program x principal 0x1 { fn main() -> unit { return; } fn other() -> unit { return; } }";
    assert_eq!(parse_source(second).unwrap_err().code, "P0011");
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythc --test parser
```

- [ ] **Step 3: Implement exact grammar**

```text
program        := "program" IDENT "principal" HEX "{" import* main_fn "}"
import         := "import" IDENT ":" "capability" "<" RESOURCE "," RIGHTS ">" ";"
main_fn        := "fn" "main" "(" ")" "->" "unit" block
block          := "{" statement* "}"
statement      := let_stmt | if_stmt | while_stmt | expression ";" | "return" ";"
let_stmt       := "let" IDENT ":" type "=" expression ";"
if_stmt        := "if" expression block ("else" block)?
while_stmt     := "while" "budget" INTEGER expression block
type           := "bool" | "u64" | "i64" | "bytes" | "utf8" | "object_id" |
                  "revision_id" | "task_id" | "proposal_id" | "capability" |
                  "error_code" | "unit"
expression     := literal | IDENT | intrinsic_call | unary | binary | "(" expression ")"
literal        := "true" | "false" | INTEGER | STRING
```

Supported binary operators: `==`, `<`, `+`, `-`, `&&`, `||`. Unary operator: `!`.

Define diagnostics:

```text
P0001 unexpected character
P0002 unterminated string
P0003 unexpected token
P0004 missing main
P0005 duplicate main
P0006 invalid principal hex
P0007 while requires literal budget
P0008 zero loop budget
P0009 unknown type spelling
P0010 duplicate import name
P0011 additional functions unsupported
```

- [ ] **Step 4: Run GREEN**

```powershell
cargo test -p pythc --test parser
```

- [ ] **Step 5: Commit**

```powershell
git add Cargo.toml tools\pythc
git commit -m "feat(pythc): parse bounded Pyth source"
```

---

### Task 2: Implement semantic type checking and intrinsic registry

**Files:**
- Create: `tools/pythc/src/types.rs`
- Create: `tools/pythc/src/intrinsics.rs`
- Create: `tools/pythc/src/typecheck.rs`
- Create: `tools/pythc/tests/typecheck.rs`

**Interfaces:**
- Produces: `TypedProgram`, exact intrinsic signatures and resource/right requirements.

- [ ] **Step 1: Write failing type tests**

```rust
#[test]
fn typechecks_object_capability_flow() {
    let typed = typecheck_source(include_str!("fixtures/object-note.pyth")).unwrap();
    assert_eq!(typed.main.result_type, PythType::Unit);
    assert!(typed.required_intrinsics.contains(&Intrinsic::ObjectCreate));
    assert!(typed.required_intrinsics.contains(&Intrinsic::ObjectRevise));
}

#[test]
fn rejects_capability_arithmetic_wrong_intrinsic_rights_and_unknown_name() {
    assert_eq!(typecheck_source(BAD_CAP_ADD).unwrap_err().code, "T0008");
    assert_eq!(typecheck_source(BAD_REVISE_RIGHTS).unwrap_err().code, "T0012");
    assert_eq!(typecheck_source(BAD_UNKNOWN_NAME).unwrap_err().code, "T0002");
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythc --test typecheck
```

- [ ] **Step 3: Define intrinsic signatures**

Use exact source names:

```text
system.log(capability, utf8) -> unit
object.create(capability, u64) -> object_id
object.created_capability() -> capability
object.created_revision() -> revision_id
object.query(capability, u64) -> object_id
object.queried_capability() -> capability
object.inspect(capability, object_id) -> utf8
object.inspected_revision() -> revision_id
object.revise(capability, object_id, u64, utf8) -> revision_id
object.history(capability, object_id) -> u64
task.active(capability) -> task_id
task.propose(capability, utf8, utf8, u64) -> proposal_id
graph.related(capability, task_id, u64) -> object_id
relevance.emit(capability, object_id, u64, utf8) -> unit
capability.request(capability, u64, u64) -> proposal_id
```

The `created_*`, `queried_*`, and `inspected_*` intrinsics are valid only immediately after their producer call in the same block. The typed AST records that producer relationship so lowering emits `HostResult` nodes.

- [ ] **Step 4: Implement symbol and rights checking**

Diagnostics:

```text
T0001 duplicate local
T0002 unknown name
T0003 type mismatch
T0004 non-bool condition
T0005 invalid return type
T0006 unsupported intrinsic
T0007 wrong argument count
T0008 capability operation forbidden
T0009 immutable local reassignment
T0010 stale host result access
T0011 import resource mismatch
T0012 import rights insufficient
T0013 integer literal overflow
T0014 loop budget exceeds 65536
```

- [ ] **Step 5: Run GREEN**

```powershell
cargo test -p pythc --test typecheck
```

- [ ] **Step 6: Commit**

```powershell
git add tools\pythc\src\types.rs tools\pythc\src\intrinsics.rs tools\pythc\src\typecheck.rs tools\pythc\tests\typecheck.rs
git commit -m "feat(pythc): typecheck Pyth programs and capabilities"
```

---

### Task 3: Lower typed AST to effect-ordered graph

**Files:**
- Create: `tools/pythc/src/graph.rs`
- Create: `tools/pythc/src/lower.rs`
- Create: `tools/pythc/src/encode.rs`
- Create: `tools/pythc/tests/lower.rs`

**Interfaces:**
- Produces: `lower_program(&TypedProgram) -> Result<OwnedGraph, Diagnostic>`, `encode_verified_graph(&OwnedGraph) -> Result<Vec<u8>, Diagnostic>`.

- [ ] **Step 1: Write failing lowering tests**

```rust
#[test]
fn lowering_builds_single_effect_chain_and_block_parameters() {
    let typed = typecheck_source(include_str!("fixtures/branch-log.pyth")).unwrap();
    let graph = lower_program(&typed).unwrap();
    let bytes = encode_verified_graph(&graph).unwrap();
    let package = PythGraphPackage::decode(&bytes).unwrap();
    let verified = verify_package(&package).unwrap();

    assert_eq!(verified.package().blocks().len(), 4);
    assert_eq!(graph.effect_forks(), 0);
    assert!(graph.contains_opcode(Opcode::Branch));
    assert!(graph.contains_opcode(Opcode::SystemLog));
}

#[test]
fn lowering_emits_explicit_budgeted_loop_back_edge() {
    let typed = typecheck_source(include_str!("fixtures/budget-loop.pyth")).unwrap();
    let graph = lower_program(&typed).unwrap();
    assert!(graph.has_back_edge());
    assert_eq!(graph.loop_budget_literals(), vec![8]);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythc --test lower
```

- [ ] **Step 3: Implement graph builder**

`OwnedGraph` owns vectors for types, blocks, nodes, imports, constants, and strings. It is host-only and never enters PythCore. Lowering rules:

- `main` creates entry block and `EffectStart`.
- Each effectful intrinsic consumes current effect node and becomes the new effect node.
- Pure expressions create value nodes.
- `if` creates then, else, and join blocks; required live values become block parameters.
- `while budget N condition` creates header, body, and exit blocks. A compiler-generated U64 counter is initialized to N and decremented on the back edge. Loop continues only when condition is true and counter is nonzero.
- `return` emits the current effect and unit result through `Return` inputs.
- Imports are sorted by source declaration order and assigned stable slots.
- Strings are deduplicated by exact bytes.

- [ ] **Step 4: Encode and invoke shared verifier**

`encode_verified_graph` encodes canonical bytes, decodes them through `PythGraphPackage::decode`, and calls `verify_package`. Any verifier rejection becomes diagnostic `G0001 shared verifier rejected compiler output` with the stable verifier error.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test -p pythc --test lower
cargo test -p pythc
```

- [ ] **Step 6: Commit**

```powershell
git add tools\pythc\src\graph.rs tools\pythc\src\lower.rs tools\pythc\src\encode.rs tools\pythc\tests\lower.rs
git commit -m "feat(pythc): lower typed source to verified graphs"
```

---

### Task 4: Add CLI, diagnostics, golden fixtures, and reproducibility

**Files:**
- Modify: `tools/pythc/src/main.rs`
- Create: `tools/pythc/src/diagnostic.rs`
- Create: `tools/pythc/tests/fixtures/hello.pyth`
- Create: `tools/pythc/tests/fixtures/object-note.pyth`
- Create: `tools/pythc/tests/fixtures/branch-log.pyth`
- Create: `tools/pythc/tests/fixtures/budget-loop.pyth`
- Create: `tools/pythc/tests/golden.rs`
- Create: `scripts/test-pythc.py`

**Interfaces:**
- Produces: `pythc check`, `pythc build`, `pythc inspect`, deterministic package bytes.

- [ ] **Step 1: Write failing CLI acceptance**

`scripts/test-pythc.py` must:

1. Compile `hello.pyth` twice and require byte-for-byte identical outputs.
2. Verify the output with `pyth-tig-tool verify`.
3. Run `pythc inspect` and require program name, principal, imports, block count, node count, checksum.
4. Compile each negative fixture and require the exact diagnostic code.
5. Print `PYTHC_TEST_OK`.

- [ ] **Step 2: Run RED**

```powershell
python scripts\test-pythc.py
```

- [ ] **Step 3: Implement CLI**

```text
pythc check <source>
pythc build <source> -o <package>
pythc inspect <package>
```

Diagnostics render:

```text
error[T0012]: import `workspace` lacks revise rights
 --> programs/examples/object-note.pyth:2:5
  |
2 |     import workspace: capability<object.workspace, create|query>;
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Exit codes:

```text
0 success
2 source/semantic diagnostic
3 package encode or verifier failure
4 I/O failure
```

- [ ] **Step 4: Run GREEN**

```powershell
python scripts\test-pythc.py
cargo test -p pythc
```

- [ ] **Step 5: Commit**

```powershell
git add tools\pythc scripts\test-pythc.py
git commit -m "feat(pythc): add deterministic compiler CLI"
```

---

### Task 5: Replace hand-built fixtures with compiled Pyth programs

**Files:**
- Create: `programs/examples/hello.pyth`
- Create: `programs/examples/object-create.pyth`
- Create: `programs/examples/object-restore.pyth`
- Modify: `scripts/build-pyth-graph.py`
- Modify: `scripts/test-pyth-graph-runtime.py`
- Modify: `scripts/test-pyth-graph-object-flow.py`

**Interfaces:**
- Produces: accepted runtime and object-flow packages from Pyth source.

- [ ] **Step 1: Write source programs**

`programs/examples/hello.pyth`:

```text
program hello principal 0x5059544847520001 {
    import log: capability<system.log, write>;
    fn main() -> unit {
        system.log(log, "hello");
        return;
    }
}
```

`object-create.pyth` and `object-restore.pyth` express the exact Phase 3 flows using compiler intrinsics and matching principal/import policy.

- [ ] **Step 2: Change build script to compiler output**

`build-pyth-graph.py` invokes:

```powershell
cargo run -p pythc -- build programs/examples/hello.pyth -o target/pyth-tig/hello.tig
cargo run -p pythc -- build programs/examples/object-create.pyth -o target/pyth-tig/object-create.tig
cargo run -p pythc -- build programs/examples/object-restore.pyth -o target/pyth-tig/object-restore.tig
```

Keep `pyth-tig-tool` hand-built fixtures only for verifier mutation tests.

- [ ] **Step 3: Run acceptance**

```powershell
python scripts\test-pythc.py
python scripts\test-pyth-graph-runtime.py
python scripts\test-pyth-graph-object-flow.py
```

Expected all three success markers.

- [ ] **Step 4: Commit**

```powershell
git add programs\examples scripts\build-pyth-graph.py scripts\test-pyth-graph-runtime.py scripts\test-pyth-graph-object-flow.py
git commit -m "feat(pythc): drive accepted graph programs from Pyth source"
```

---

## Phase 4 verification

```powershell
cargo fmt --all -- --check
cargo test -p pythc
cargo clippy -p pythc --all-targets -- -D warnings
python scripts\test-pythc.py
python scripts\test-pyth-tig-format.py
python scripts\test-pyth-graph-runtime.py
python scripts\test-pyth-graph-object-flow.py
python scripts\test-object-shell.py
python scripts\test-boot.py
```

Dispatch the compiler reviewer using `prompts/compiler-reviewer.md`. Require explicit confirmation that compiler typing, opcode signatures, effect chains, import rights, and shared verifier semantics agree.
