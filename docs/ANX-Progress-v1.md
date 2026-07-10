# ANX — Progress Tracker (v1)

**Companion to:** [ANX-Implementation-Plan-v1.md](ANX-Implementation-Plan-v1.md), [ANX-Tech-Stack-v1.md](ANX-Tech-Stack-v1.md), [ANX-Usage-Flow-v1.md](ANX-Usage-Flow-v1.md)
**Purpose:** living record of what's actually been built, phase by phase, against the Implementation Plan's exit gates — check this before assuming any phase's state; it's the source of truth over memory of past conversations.

**How to update:** when a phase's exit gate is met, flip its row in the table below, fill in its detail section, and append one line to the changelog at the bottom with the date and commit hash.

---

## Status at a glance

| Phase | Status | Exit gate met? | Commit(s) |
|---|---|---|---|
| 0 — Bootstrap | ✅ Done | Yes | `fa894aa` |
| 1 — Lexer | ✅ Done | Yes | `0954b6f` |
| 2 — AST & Parser | ✅ Done | Yes | `aaf0904` |
| 3 — Semantic Analysis | ✅ Done | Yes | `309f197` |
| 4 — Tree-Walking Interpreter | ✅ Done | Yes | `dc1180b` |
| 5 — LLVM Codegen | ✅ Done | Yes | `7293657` |
| 6 — Compile Pipeline & CLI | ✅ Done | Yes | `0252e9a` |
| 7 — Benchmark Suite (20 problems) | ✅ Done | Yes | *(uncommitted)* |
| 8 — Dogfooding (10 real problems) | ⬜ Not started | — | — |

---

## Phase details

### Phase 0 — Bootstrap ✅
- Cargo project initialized in place (`cargo init --name anx`); module skeleton (`lexer/`, `ast/`, `parser/`, `sema/`, `interp/`, `codegen/`, `cli.rs`) created per the Implementation Plan's project structure.
- Dependencies: `clap` 4 (derive), `thiserror` 2.0.18, `anyhow` 1.0.103, `inkwell` 0.9.0 (feature `llvm21-1`); dev-deps `assert_cmd` 2.2.2, `predicates` 3.1.4.
- LLVM 21 installed via Homebrew, `LLVM_SYS_211_PREFIX` exported in `~/.zshrc`.
- `cargo build` and `cargo test` both green with the skeleton in place.
- Commit: `fa894aa` (pushed to `origin/main`).

### Phase 1 — Lexer ✅
- Hand-written scanner in `src/lexer/mod.rs` / `src/lexer/token.rs` — full P0 token set (literals, keywords, operators, punctuation), line/col tracking on every token, both comment styles.
- 18 unit tests: one per token category, comment-skipping, escape sequences, error cases (`LexError` via `thiserror`), and both worked examples from the syntax draft.
- **Exit gate** ("binary-search worked example lexes with no errors"): met.
- Commit: `0954b6f` (pushed to `origin/main`).
- Deferred, not gaps: P1/P2 keywords (`class`/`interface`/`try`/`catch`/`this`) aren't reserved yet — they lex as plain identifiers until those phases need them; no unary-minus token (negation is a parser concern); lexer halts at the first error rather than collecting all (matches parser's planned behavior — only sema collects everything).

### Phase 2 — AST & Parser ✅
- AST node types in `src/ast/mod.rs`: `Program`/`Decl` (`VarDecl`, `FuncDecl`), `Stmt` (incl. `IfStmt`/`WhileStmt`/`ForStmt`/`ReturnStmt`/`Block`), `Expr` (literals, `Ident`, `Binary`/`Unary`, `Assign`, `Call`, `Index`, `ArrayLiteral`, `ArrayCreation`, `FieldAccess`), plus `Type`/`BinOp`/`UnOp`. Every node carries a `NodeId` (`u32` counter) for sema's future side-table annotations.
- Recursive-descent parser in `src/parser/mod.rs` implementing the syntax draft's grammar plus the `new type[expr]` array-creation addition from the Implementation Plan §1. Expression parsing is precedence-climbing per the plan's table (assignment < `||` < `&&` < equality < comparison < term < factor < unary < postfix).
- Two necessary generalizations beyond the syntax draft's informal grammar (documented in code comments, not silent deviations):
  - **`Expr::FieldAccess`** added for `arr.length` — the grammar sketch never spelled out field/property access as its own production, but the lexer already carries a `Dot` token for exactly this.
  - **Assignment target generalized from `IDENTIFIER` to any postfix expression** — the informal grammar (`assignment ::= IDENTIFIER "=" assignment | ...`) only allows a bare identifier on the left of `=`, but every in-place sorting benchmark (bubble/insertion/merge/quicksort) needs `arr[j] = tmp;`. The parser now accepts any expression as an assignment target syntactically; validating it's actually an lvalue (rejecting e.g. `5 = x;`) is deferred to Phase 3 sema, consistent with how the plan treats syntax vs. semantic concerns elsewhere.
- Parser error recovery: minimal, first error wins, reported with line number (`ParseError` via `thiserror`) — matches the plan's P0 approach.
- 18 unit tests: per-construct AST-shape assertions (var decl, arrays, array creation, recursion, if/else-if/else, while, for, lvalue assignment, right-associativity, field access, precedence), 2 error-case tests, and the full variables/arrays/functions/control-flow/binary-search samples from the syntax draft parsed end-to-end.
- **Exit gate** ("every code sample in the syntax draft — excluding classes/collections/try-catch — parses without error"): met.
- Commit: `aaf0904` (pushed to `origin/main`).

### Phase 3 — Semantic Analysis ✅
- Symbol table (`src/sema/symtab.rs`): scope stack + a separate function-signature table, supporting push/pop scope, var declare/resolve (innermost-first), function declare/resolve.
- Two-pass driver (`src/sema/mod.rs`): pass 1 hoists every function signature (so forward calls and recursion resolve) and checks a valid `void main()` exists; pass 2 walks declarations in source order, type-checking bodies and control-flow return-completeness.
- Type-checking rules (`src/sema/types.rs`): expression type inference (`check_expr`) plus a contextual variant (`check_expr_against`) used wherever an expected type is already known (var decl initializers, call arguments, assignment RHS) — needed so array literals check element-by-element against the expected element type rather than only inferring from the first element.
- AST annotated in place via two side tables per the Implementation Plan's shared-sema design: `SemaResult.types: HashMap<NodeId, Type>` (every expression's resolved type) and `SemaResult.var_refs: HashMap<NodeId, SymbolId>` (identifier → declaring symbol).
- Error collection: unlike the lexer/parser, sema does **not** stop at the first error — every error in a pass is collected into `Vec<SemaError>`, per the plan's explicit requirement.
- 16 sema unit tests (6 valid-program cases, 10 invalid-program cases each asserting the specific error variant) plus 20 tests type-checking the actual Phase 7 benchmark programs (see below).
- **All 20 P0 benchmark programs were written now** (`tests/benchmarks/*.nx`) — earlier than Phase 7's own file-creation step — because Phase 3's exit gate requires them to exist and type-check; this is deliberate, catching any P0 grammar/sema gaps against real DSA solutions before the interpreter or LLVM backend get built on top. Algorithmic correctness was verified by hand (no interpreter exists yet to execute them) — runtime confirmation is Phase 4/7's job. Covers: binary search (+first/last occurrence), two-pointer (two-sum, reverse, dedupe), all four classic sorts, merge sort's recursive scratch-buffer allocation, quicksort's in-place partition, 5 recursion problems (factorial, naive/memoized fibonacci, fast exponentiation, GCD), and 4 DP problems (climbing stairs, coin change, 0/1 knapsack via a flattened 1D array, LIS, Kadane's).
- **Exit gate** ("all 20 benchmark programs type-check with zero errors; each invalid-program test produces exactly the expected error category"): met.
- Commit: `309f197` (pushed to `origin/main`).

### Phase 4 — Tree-Walking Interpreter ✅
- `Value` enum (`src/interp/value.rs`): `Int`/`Float`/`Bool`/`Str`/`Array(Rc<RefCell<Vec<Value>>>)`/`Void`, plus pure helpers (`default_value`, `format_value`, `numeric_op`/`numeric_cmp`) kept separate from the evaluator itself.
- `Environment` (`src/interp/mod.rs`): parent-linked scope chain via `Rc<Environment>`, mirroring sema's scope-stack structure exactly (same `exec_scoped` wrapping pattern as sema's `check_scoped_stmt`) so behavior can't diverge from what type-checked.
- Direct AST walk, no bytecode layer. Functions get a fresh frame parented to *globals* (not the caller's locals) on every call — ANX functions aren't closures, matching sema's per-function scoping. Recursion rides Rust's own call stack.
- Control flow (`Flow::Normal` / `Flow::Return(Value)`) threads early return up through nested blocks/loops without exceptions.
- `print()` routes through an injectable output sink (a closure) rather than a hardcoded `println!` — real usage defaults to stdout, but this is what let tests assert on captured program output directly, without needing Phase 6's CLI or a subprocess.
- Runtime errors (`RuntimeError`, via `thiserror`) for the two cases the plan calls out: array index out of bounds and int division/modulo by zero — plus one more found while implementing this phase: negative `new int[n]` size. These return `Err`, not a panic; every other failure mode is asserted as sema-impossible (`unreachable!`/`panic!` with a comment naming the guarantee), since the interpreter only ever runs sema-validated programs.
- **Fixed a real cross-phase bug found while designing this phase**: sema's `is_lvalue` previously accepted `Expr::FieldAccess` as a valid assignment target, meaning `arr.length = 5;` would type-check — but `.length` is a computed property with nothing to actually assign to. Narrowed `is_lvalue` to `Ident`/`Index` only, added a sema regression test.
- 14 construct-level unit tests (arithmetic, comparisons, `&&`/`||` short-circuiting verified via a side-effecting stub that must *not* run, if/else-if/else, while, for, recursion, array reference semantics across a function call, `new int[n]` zero-initialization) plus 4 runtime-error tests, plus all 20 benchmark programs run end-to-end.
- **All 20 benchmark expected outputs were hand-traced** (documented per-program in the test file) and matched exactly on the first run — this cross-validates both the manual algorithm tracing from Phase 3 and the interpreter's correctness, and effectively produces the `.expected` content Phase 7 will formalize into files.
- **Exit gate** ("all 20 benchmark programs produce correct output when run through the interpreter"): met.
- Commit: `dc1180b` (pushed to `origin/main`).

### Phase 5 — LLVM Codegen ✅
- `Codegen<'ctx>` (`src/codegen/mod.rs`) using inkwell 0.9.0 / LLVM 21: one `Context`/`Module`/`Builder` per compilation, scoped-alloca variable tracking mirroring sema/interp's scope-stack pattern exactly, entry-block alloca placement (standard practice so loop bodies never grow the stack per iteration).
- Runtime declarations (`src/codegen/runtime.rs`) for libc `malloc`/`calloc` plus five C shim functions; the actual C implementations live in `src/codegen/runtime.c` (not yet compiled/linked anywhere — that's Phase 6).
- Control flow lowered to basic blocks with explicit "did this block terminate" tracking threaded through if/while/for, matching sema's return-completeness analysis exactly (an if/else where both branches return correctly propagates as terminated; the resulting unreachable merge block still gets an `unreachable` terminator so the module verifies).
- **Two deliberate design changes from this doc's original Phase 5 sketch, made during implementation and worth flagging**:
  - **Arrays are by-value `{i64 length, ptr data}` structs, not pointers-to-a-heap-allocated-struct.** LLVM 21's opaque pointers mean `data`'s LLVM type never encodes the element type anyway, so there's no benefit to heap-allocating the wrapper — passing the small struct by value (copying the `length`/`data` pair) still gives correct reference semantics for the underlying array, since the copied `data` pointer targets the same malloc'd buffer. Only the data buffer itself is heap-allocated. `new int[n]` uses `calloc` (auto-zeroing) instead of a manual zero-fill loop.
  - **Bool is `i1` everywhere**, not "i1 widened to i8 for storage" as originally sketched — LLVM allows `i1` allocas/loads/stores/array-elements directly; the widen-to-i8 only happens at the one place C's ABI actually needs it, the `anx_print_bool` call boundary.
  - Because opaque pointers erase element-type information from the LLVM value itself, codegen is the first pass that actually needs to consult sema's `SemaResult.types` side table (the interpreter never needed it, since Rust's own runtime `Value` enum self-describes) — this is the concrete payoff of the shared-AST/side-table design from Implementation Plan §1.
- 20 construct-level tests (`Module::verify()` only) plus all 20 benchmark programs emitting verifiable IR — Phase 5's literal exit gate.
- **Went beyond the exit gate**: `Module::verify()` only checks IR is well-formed, not logically correct. Added 6 JIT-execution tests (via inkwell's `create_jit_execution_engine`) that actually *run* compiled functions and check results — factorial(5)=120, fibNaive(10)=55, gcd(48,18)=6, power(2,10)=1024, climbStairs(5)=8 (exercises `new int[n]`/calloc/GEP internally), Kadane's max-subarray=6 (exercises array literals + while loop). JIT tests are restricted to scalar-only function signatures — an array *parameter* is a by-value LLVM struct whose C ABI lowering isn't safe to hand-match from a raw Rust function pointer; functions using arrays only as internal locals avoid that risk while still exercising the array logic.
- **Exit gate** ("every construct in the P0 feature list emits verifiable LLVM IR"): met, and additionally verified logically correct on 6 representative cases via JIT.
- Commit: `7293657` (pushed to `origin/main`).

### Phase 6 — Compile Pipeline & CLI ✅
- `src/cli.rs`: `clap`-derived `anx check|run|build` subcommands over a shared `frontend()` (lex → parse → sema) helper, so error reporting/exit codes are identical across all three paths.
- Exit codes match `docs/ANX-Usage-Flow-v1.md` exactly: `0` success, `1` any compile-time error (lex/parse/sema, reported with `{file}:{line}: {message}`), `2` runtime error.
- **`anx build`**: `TargetMachine::write_to_file` emits a native object file, then a fresh `anx_runtime.c` (the shim, `include_str!`'d into the `anx` binary itself — a built `anx` needs no companion files, only a system C compiler) is written alongside it and both are hand-and-linked in one `cc` invocation, which pulls in libc and the platform's C startup code.
- **ANX's `void main()` is emitted as the LLVM symbol `anx_main`, with a generated C-ABI `int main()` wrapper calling it and returning 0** — necessary because a native binary's real entry point must be an `int`-returning C `main`, which the language's own `void main()` convention (Usage Flow doc) can't be directly. The `functions` map still keys by the ANX-source name `"main"`, so this is invisible to the rest of codegen.
- **Added the three runtime guards Phase 5 deferred**: LLVM codegen now emits explicit branches for array-bounds (`icmp ult` against length — a negative index wraps to a huge unsigned value, so one unsigned compare covers both bounds), int div/mod-by-zero, and negative `new int[n]` size, each branching to a call into one of three new `runtime.c` panic functions (`anx_panic_oob`/`anx_panic_div_zero`/`anx_panic_neg_size`) that print to stderr and `exit(2)`. Without these, a compiled binary would silently corrupt memory or hit LLVM-level UB on the exact cases the interpreter already catches cleanly — the plan's "both paths must behave identically" bar (Usage Flow doc) doesn't hold without them. JIT tests from Phase 5 map these three symbols to Rust stub functions (via `ExecutionEngine::add_global_mapping`) since the C shim isn't linked into the test binary; the stubs panic if actually hit, since JIT happy-path tests should never reach them.
- 8 CLI integration tests (`tests/cli.rs`, `assert_cmd` + `predicates`, per the Tech Stack doc): `check`/`run` accept/reject valid and invalid programs with the right exit code and stderr content, `run` surfaces a runtime error at exit 2, `build` refuses to produce a binary on a compile error, and — the plan's literal exit gate — building `01_binary_search.nx` and executing the *resulting binary directly* (not through `anx`) produces the correct output, proving it's a genuine standalone executable per PRD Goal 3. One more test confirms the compiled path's new runtime guards produce the same exit-2 behavior as the interpreter.
- **Went beyond the exit gate**: manually built and ran all 20 benchmark programs through the compiled path and diffed against the interpreter's output for each — all 20 match exactly (not yet a committed automated test; the full dual-path suite with `.expected` fixtures is Phase 7's job, and this doc's own precedent of front-running phases was intentionally *not* repeated here since Phase 6's exit gate doesn't require it). Also manually confirmed all three runtime guards fire correctly in real compiled binaries with the interpreter's exact error text.
- **Exit gate** ("`anx build` on the binary-search benchmark produces a standalone native binary that runs and produces correct output, verifiable with `file`"): met.
- Commit: `0252e9a` (pushed to `origin/main`).

### Phase 7 — Benchmark Suite ✅
- All the substantive work here was already done in earlier phases (the 20 `.nx` programs in Phase 3, the hand-traced expected outputs cross-validated by the interpreter in Phase 4, the compiled-path match manually confirmed in Phase 6) — this phase's job was formalizing that into standing, committed fixtures and an automated test rather than a one-time manual check.
- Added `tests/benchmarks/NN_name.expected` for all 20 programs, values taken directly from the already-verified `benchmark_produces_output!` macro invocations in `src/interp/mod.rs` (Phase 4) rather than re-derived, to avoid a transcription error silently diverging from what was actually proven correct.
- `tests/integration.rs`: one test that, for every benchmark, runs it through `anx run` (spawned via `assert_cmd`) *and* builds + directly executes the resulting binary (via `anx build` then `std::process::Command` on the output path, same pattern as `tests/cli.rs`'s standalone-binary check), diffing both against the paired `.expected` file. Collects every mismatch before failing (rather than stopping at the first) so a regression run shows the full picture in one go.
- **All 20 programs pass on both the interpreter and the compiled path** — this is simultaneously Phase 7's exit gate and the PRD's actual leading success metric ("% of the 20-problem benchmark suite that compiles and runs correctly — target 100% on the P0 feature set before calling v1 done"). **This completes the entire P0 milestone** (PRD Goal 1); only Phase 8 (Dogfooding — a usage phase, not further coding) remains before v1 is done.
- **Exit gate** ("100% of the 20 programs pass on both the interpreter and the compiled path"): met.
- Not yet committed.

### Phase 8 — Dogfooding ⬜
Not started. 0/10 real DSA problems solved in ANX.

---

## Changelog

- **2026-07-08** — Phase 0 (Bootstrap) complete. Commit `fa894aa`, pushed.
- **2026-07-08** — Phase 1 (Lexer) complete. Commit `0954b6f`, pushed.
- **2026-07-08** — Phase 2 (AST & Parser) complete. Commit `aaf0904`, pushed.
- **2026-07-08** — Phase 3 (Semantic Analysis) complete, incl. writing all 20 P0 benchmark `.nx` programs ahead of schedule. Commit `309f197`, pushed.
- **2026-07-08** — Phase 4 (Tree-Walking Interpreter) complete; all 20 benchmarks produce hand-verified correct output. Also fixed a sema bug (`arr.length` wrongly accepted as an assignment target) found while designing this phase. Commit `dc1180b`, pushed.
- **2026-07-08** — Phase 5 (LLVM Codegen) complete; all 20 benchmarks emit verifiable IR, plus 6 JIT-executed correctness checks beyond the literal exit gate. Revised the array runtime layout (by-value struct, not heap pointer-to-struct) and bool representation (`i1` throughout) from this doc's original sketch, based on what LLVM 21's opaque pointers actually make simplest. Commit `7293657`, pushed.
- **2026-07-08** — Phase 6 (Compile Pipeline & CLI) complete. `anx check|run|build` all working with matching exit codes (0/1/2); `anx build` links a C runtime shim compiled fresh each run, and now also emits the array-bounds/div-zero/negative-size runtime guards Phase 5 deferred, so the compiled path fails identically to the interpreter. All 20 benchmarks manually verified to match between interpreter and compiled binary. Commit `0252e9a`, pushed.
- **2026-07-08** — Phase 7 (Benchmark Suite) complete: formalized the already-verified 20 programs into `.expected` fixtures + `tests/integration.rs`. All 20 pass on both interpreter and compiled path — **this is the PRD's leading success metric and completes the entire P0 milestone.** Only Phase 8 (Dogfooding) remains. Not yet committed.
