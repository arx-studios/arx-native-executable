# ANX — Implementation Plan (v1)

**Companion to:** [ANX-PRD-v1.md](ANX-PRD-v1.md), [ANX-Syntax-Draft-v1.md](ANX-Syntax-Draft-v1.md), [ANX-Usage-Flow-v1.md](ANX-Usage-Flow-v1.md), [ANX-Tech-Stack-v1.md](ANX-Tech-Stack-v1.md), [ANX-Progress-v1.md](ANX-Progress-v1.md)
**Scope:** P0 only (frontend → interpreter → LLVM backend → 20-problem benchmark suite). P1 (collections/classes) is staged at the end for continuity but is explicitly out of v1 scope per the PRD.
**Host language for the compiler itself:** Rust.

---

## 1. Key engineering decisions

The PRD flags three blocking/non-blocking open questions. Resolving them up front so implementation isn't blocked mid-phase:

| Question | Decision | Rationale |
|---|---|---|
| Memory model (blocking) | **Arena / "leak on exit."** All heap allocations (arrays) use `malloc` and are never freed; the OS reclaims on process exit. No GC, no borrow-checked ownership in the *target* language. | ANX programs are short-lived DSA scripts (seconds of runtime, small inputs). This is the simplest model that's still "real" — avoids needing a GC runtime or Rust-style ownership semantics baked into ANX's syntax, either of which would be a large P0 detour. Revisit only if P1 classes introduce long-lived linked structures where leaking meaningfully matters (unlikely at DSA-practice scale). |
| Shared interpreter/codegen semantics (blocking) | **One AST, one semantic analysis pass.** Sema annotates the AST in place (types, resolved symbol references) rather than producing a second "typed AST" data structure. Both the interpreter and codegen walk the *same* annotated tree. | Directly serves PRD Goal 5 (shared architecture, no rewrite for P1). Divergent ASTs would double the maintenance surface for a solo dev. |
| Generics strategy (non-blocking) | Deferred to P1 as stated. Flagging now that monomorphization (stamp out a concrete `Stack_int`, `Stack_string`, etc. per instantiation) is the likely direction, since it codegens to plain LLVM structs/functions with no runtime type tag needed — fits the "leak on exit," no-GC model better than type erasure would. | No P0 work required; recorded so the P1 design doesn't start from zero. |

One gap in the syntax draft that needs closing before codegen can start: **the grammar has no way to allocate an array whose size isn't a compile-time literal.** Merge sort's scratch buffer, DP tables (coin change, knapsack, LIS), etc. all need `new int[n]` where `n` is a runtime value. Array literals (`[1,2,3]`) alone can't express this. Adding one production closes the gap without touching anything else in the draft:

```
arrayCreation ::= "new" type "[" expr "]"
```

`int[] scratch = new int[n];` — elements default-initialized to `0` / `false`. This is additive to the existing grammar sketch, not a revision of it.

Two-dimensional DP tables (knapsack) are handled by **flattening to 1D** (`row * width + col` indexing) rather than adding `int[][]` to the type grammar — keeps the "minimal v1 language surface" goal intact per PRD Goal 2.

`string` is scoped down for P0 to **literals only**, used exclusively as arguments to `print()` (e.g. `print("positive")`). No concatenation, indexing, or mutation semantics are implemented — those, along with `try`/`catch`, are not in the PRD's P0 requirements at all and are left where the syntax draft already parks them (sketched, not built).

A second gap: **none of the syntax draft's examples define a program entry point.** Every example is just a bag of function declarations with no indication of what runs first. Resolved as: a top-level `void main()` function is **required** in every `.nx` file and is what both `anx run` and a built binary execute first — same convention as C/Java, simplest possible choice, no new syntax needed (it's just a function named `main`). Full user-facing detail (file extension, CLI flow, exit codes) is in the companion [ANX-Usage-Flow-v1.md](ANX-Usage-Flow-v1.md).

---

## 2. Project structure

A single Rust crate with clear module boundaries (not a multi-crate workspace — the build/publish overhead of separate crates isn't worth it at this scale for a solo project; modules give the same separation and can be split into crates later if that ever becomes necessary):

```
anx/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lexer/
│   │   ├── mod.rs
│   │   └── token.rs
│   ├── ast/
│   │   └── mod.rs           # Node types, shared by parser, sema, interp, codegen
│   ├── parser/
│   │   └── mod.rs           # Recursive descent + Pratt parsing for expressions
│   ├── sema/
│   │   ├── mod.rs
│   │   ├── symtab.rs        # Scoped symbol table
│   │   └── types.rs         # Type representation + type-checking rules
│   ├── interp/
│   │   ├── mod.rs
│   │   └── value.rs         # Runtime Value enum
│   ├── codegen/
│   │   ├── mod.rs           # inkwell-based LLVM IR emission
│   │   └── runtime.rs       # malloc-backed array layout, builtins (print)
│   └── cli.rs               # `anx run|build|check` subcommands
├── tests/
│   ├── benchmarks/           # the 20 .nx programs (see §6)
│   │   ├── 01_binary_search.nx
│   │   ├── 01_binary_search.expected
│   │   └── ...
│   └── integration.rs        # runs every benchmark through both backends
└── docs/
    └── ANX-Implementation-Plan-v1.md   # this file
```

**Toolchain (macOS/Darwin, matches current environment):**
```
brew install llvm@21
export LLVM_SYS_211_PREFIX=$(brew --prefix llvm@21)
cargo add inkwell --features llvm21-1
```
(inkwell supports LLVM versions 11 through 21 via `llvmM-N` feature flags — pinned to the current release, 21, rather than an older one; no compatibility reason to lag behind.)

---

## 3. Phase-by-phase plan

Each phase has a concrete exit gate — don't move to the next until it's met. Sequencing follows the PRD's suggested order exactly: frontend → interpreter → codegen → benchmark suite → (P1, staged separately).

### Phase 0 — Bootstrap
- `cargo new anx`, set up the module skeleton above with empty stubs.
- Add `clap` (CLI arg parsing), `thiserror`/`anyhow` (error handling), `inkwell` (LLVM bindings).
- Set up `cargo test` to run and pass on an empty test.
- **Exit gate:** `cargo build` and `cargo test` succeed with the skeleton in place.

### Phase 1 — Lexer
- Token set: literals (`int`, `float`, `bool`, `string`), identifiers, keywords (`if/else/while/for/return/true/false/null/new/void` + primitive type names), operators (`+ - * / % == != < <= > >= && || ! = [ ] ( ) { } , ;`), comments (`//`, `/* */`).
- Hand-written lexer (`Vec<char>` or byte-slice scanning) producing `Vec<Token>` with source line/column info attached to every token — line numbers are needed for later error messages even though the PRD only requires "clearer" errors in P1; attaching them now is nearly free and expensive to retrofit.
- Unit tests: one test per token category, plus a test lexing each worked example already in the syntax draft (binary search, the control-flow snippet).
- **Exit gate:** the binary-search worked example from the syntax draft lexes into a token stream with no errors.

### Phase 2 — AST & Parser
- Define AST node types in `ast/mod.rs`: `Program`, `Decl` (`VarDecl`, `FuncDecl`), `Stmt` (`ExprStmt`, `If`, `While`, `For`, `Return`, `Block`), `Expr` (`Literal`, `Ident`, `Binary`, `Unary`, `Call`, `Index`, `ArrayLiteral`, `ArrayCreation`, `Assign`).
- Give every node a stable `NodeId` (simple incrementing counter) — this is what sema will use to attach type annotations without a second tree.
- Recursive-descent parser for statements/declarations; Pratt (precedence-climbing) parser for expressions, with precedence table: `= ` (right-assoc) < `||` < `&&` < `== !=` < `< <= > >=` < `+ -` < `* / %` < unary `! -` < postfix `[] ()`.
- Implement the grammar sketch from the syntax draft plus the `new type[expr]` addition from §1.
- Parser error recovery: minimal for P0 — report first error with line number and stop (matching "compiler correctness first, nice errors later" framing in the PRD).
- Unit tests: parse each construct in the syntax draft into an expected AST shape; parse the binary-search example end-to-end.
- **Exit gate:** every code sample in the syntax draft (excluding classes/collections/try-catch, which are explicitly deferred) parses without error.

### Phase 3 — Semantic Analysis (shared pass)
- Symbol table: scope stack (`Vec<HashMap<String, Symbol>>`), pushed/popped per block/function.
- Two-pass per function: (1) hoist function signatures so forward calls and recursion resolve, (2) walk bodies resolving identifiers and checking types.
- Type checking rules: no implicit int↔float coercion beyond literal contexts (keep simple — explicit casts can be a P1/P2 nicety); array element type must match declared `type[]`; array index must be `int`; `if`/`while` conditions must be `bool`; function return type must match all `return` statements; recursive calls type-check against the (already-hoisted) signature.
- Annotate the AST in place via a side table `HashMap<NodeId, ResolvedType>` plus `HashMap<NodeId, SymbolId>` for resolved identifier references — this is the concrete mechanism for the "one AST, shared sema" decision in §1.
- Error reporting: collect all errors per pass (don't stop at first) and report with line numbers before exiting non-zero.
- Unit tests: a battery of valid programs (should type-check clean) and invalid ones (undeclared identifier, type mismatch in assignment, wrong arg count/type on a call, non-bool `if` condition, array index with non-int) each asserting the correct diagnostic.
- **Exit gate:** all 20 benchmark programs (written but not yet executed — see Phase 7) type-check with zero errors; each invalid-program test produces exactly the expected error category.

### Phase 4 — Tree-Walking Interpreter
- `Value` enum: `Int(i64)`, `Float(f64)`, `Bool(bool)`, `Str(String)`, `Array(Rc<RefCell<Vec<Value>>>)`, `Void`.
- `Environment`: scope-chained variable bindings (`Rc<RefCell<...>>` parent pointers), matching the sema-established scoping rules exactly so behavior can never diverge between "type-checks" and "runs."
- Evaluator walks the annotated AST directly (no separate typed-AST — consumes the same side tables sema produced).
- Function calls: new environment frame per call, parameters bound, recursion works via normal Rust call-stack recursion (acceptable for DSA-sized inputs; no manual stack management needed for a tree-walker).
- `print()` builtin wired to stdout.
- Runtime errors (array out-of-bounds, division by zero) → clean error message + non-zero exit, not a Rust panic.
- **Exit gate:** all 20 benchmark programs produce correct output when run through the interpreter (`anx run`).

### Phase 5 — LLVM Codegen
- Set up `inkwell::context::Context`, one `Module` per compiled file, one `Builder` (against LLVM 21, per §2).
- Type mapping: `int` → `i64`, `float` → `double`, `bool` → `i1` (widened to `i8` for storage), `string` literal → global `i8*` constant (read-only, print-only per §1).
- Array runtime layout: a struct `{ i64 length, T* data }` where `data` is `malloc`'d (`i64 * sizeof(T)` bytes) and never freed (§1 memory model). Array literals emit a malloc + sequence of stores; `new type[n]` emits a malloc sized by the runtime value `n` plus a zero-fill loop.
- Control flow via basic blocks: `if/else` → cond-br to then/else/merge blocks; `while`/`for` → cond-br loop header/body/exit blocks, matching standard LLVM structured-control-flow lowering.
- Functions: one LLVM function per ANX function, direct recursive calls (LLVM/the target's call stack handles recursion — no CPS or trampolining needed).
- `print()` builtin: declare and call into a small C runtime shim (`anx_print_int`, `anx_print_bool`, `anx_print_str`, `anx_print_array`) implemented in a tiny bundled `.c` file compiled once and linked in — simplest way to get real stdout output out of a native binary without hand-rolling `printf` format strings in IR.
- Unit tests: for each language construct, emit IR and assert it verifies (`Module::verify()`) — don't yet require running the binary at this stage, just valid IR.
- **Exit gate:** every construct in the P0 feature list (variables, arithmetic/logic, if/else/while/for, functions, recursion, arrays) emits verifiable LLVM IR.

### Phase 6 — Compile Pipeline & CLI
- `anx check <file>` — lex/parse/sema only, report errors, exit 0/1.
- `anx run <file>` — full pipeline through the Phase 4 interpreter.
- `anx build <file> -o <output>` — lex/parse/sema → Phase 5 codegen → LLVM's `TargetMachine` emits an object file → shell out to the system linker (`cc`) to link the object file with the runtime shim from Phase 5 into a native executable.
- **Exit gate:** `anx build tests/benchmarks/01_binary_search.nx -o /tmp/bs && /tmp/bs` runs and produces correct output as a genuine standalone native binary (verifiable with `file /tmp/bs`).

### Phase 7 — Benchmark Suite (P0 exit gate for the whole project)
20 programs, deliberately chosen to need only primitives/arrays/recursion (no collections), per PRD requirement:

1. Binary search
2. Binary search — first/last occurrence of a target
3. Two-sum on a sorted array (two-pointer)
4. Reverse an array in place (two-pointer)
5. Remove duplicates from a sorted array in place (two-pointer)
6. Bubble sort
7. Insertion sort
8. Selection sort
9. Merge sort (recursive, uses `new int[n]` scratch buffer)
10. Quicksort (recursive, in-place partition)
11. Factorial (recursion)
12. Fibonacci — naive recursive
13. Fibonacci — memoized via array (bridges recursion + arrays)
14. Fast exponentiation (`power(base, exp)`, recursion)
15. GCD (Euclidean algorithm, recursion)
16. Climbing stairs (DP, 1D array)
17. Coin change — minimum coins (DP, 1D array)
18. 0/1 knapsack (DP, flattened 2D → 1D array)
19. Longest increasing subsequence (DP, 1D array)
20. Maximum subarray sum — Kadane's algorithm

Each gets: `NN_name.nx` (source), `NN_name.expected` (expected stdout). `tests/integration.rs` runs every file through **both** `anx run` and a built-then-executed `anx build` binary, diffing stdout against `.expected` for each.
- **Exit gate (= PRD's P0 done criterion):** 100% of the 20 programs pass on both the interpreter and the compiled path.

### Phase 8 — Dogfooding
- Not a coding phase — a usage phase. Solve ≥10 *real* DSA practice problems (distinct from the fixed benchmark suite) in ANX during placement prep, per PRD Goal 4 / success metric.
- Track friction points (missing syntax, confusing errors, awkward patterns) as they come up — this is the primary input for what P1 actually needs to prioritize, beyond the PRD's current P1 list.
- **Exit gate:** 10 problems solved without falling back to Java; friction notes captured somewhere durable (even a running notes file) to seed the P1 plan.

---

## 4. P1 staging (out of v1 scope — recorded for continuity only)

Not part of this plan's execution, per the PRD's non-goals/timeline. Noted so Phase 3's "shared sema" design can be sanity-checked against what's coming:
- Built-in `List`/`Stack`/`Queue`/`HashMap`, generic via monomorphization (§1).
- Classes: fields/constructors/methods — will need the symbol table (Phase 3) extended with a type namespace, and codegen (Phase 5) extended with struct types + method-call lowering (static dispatch only; no interfaces/vtables until P2).
- `Tree`/`Graph` as standard-library types built on top of classes + collections, not new primitives.
- Better compiler diagnostics — the line/column info already threaded through since Phase 1 makes this mostly a message-quality pass, not new infrastructure.

---

## 5. Testing strategy summary

- **Unit tests** per phase (lexer tokens, parser AST shapes, sema pass/fail cases, interpreter values, codegen IR verification) — fast, run on every change.
- **Integration/golden tests** (Phase 7) — the 20-program benchmark suite run through both backends, this is the PRD's actual leading success metric and should be the thing CI (even if that's just a local `cargo test` habit, given no external deadline) gates on.
- No fuzzing/property testing in v1 — not proportionate to a solo-dev, single-user-validated P0.

## 6. Risks & mitigations

| Risk | Mitigation |
|---|---|
| LLVM/inkwell version drift or install friction on macOS | Pin one LLVM version (21) in this doc and `Cargo.toml` up front; document the exact `brew`/env-var setup in §2 so there's no ambiguity later. |
| Recursion depth blowing the native (or tree-walker's Rust) call stack on deep DP/recursive benchmarks | Benchmark inputs are DSA-practice-sized (small N); not a concern at this scale. Revisit only if dogfooding (Phase 8) surfaces a real case. |
| Scope creep into P1 features mid-benchmark-suite (e.g. wanting `HashMap` for a "cleaner" DP solution) | The 20-problem list in §Phase 7 is deliberately chosen to be array/recursion-only — resist substituting a collection-based solution; that's exactly the boundary the PRD draws for P0. |
| Solo-dev bandwidth (nights/weekends, alongside placement prep/capstone/internship) | Phase boundaries above are each independently shippable/testable — safe to pause between any two phases without losing a coherent state. |
