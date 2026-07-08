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
| 4 — Tree-Walking Interpreter | ✅ Done | Yes | *(uncommitted)* |
| 5 — LLVM Codegen | ⬜ Not started | — | — |
| 6 — Compile Pipeline & CLI | ⬜ Not started | — | — |
| 7 — Benchmark Suite (20 problems) | ⬜ Not started | — | — |
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
- Not yet committed.

### Phase 5 — LLVM Codegen ⬜
Not started.

### Phase 6 — Compile Pipeline & CLI ⬜
Not started.

### Phase 7 — Benchmark Suite ⬜
Not started. 0/20 benchmark problems passing.

### Phase 8 — Dogfooding ⬜
Not started. 0/10 real DSA problems solved in ANX.

---

## Changelog

- **2026-07-08** — Phase 0 (Bootstrap) complete. Commit `fa894aa`, pushed.
- **2026-07-08** — Phase 1 (Lexer) complete. Commit `0954b6f`, pushed.
- **2026-07-08** — Phase 2 (AST & Parser) complete. Commit `aaf0904`, pushed.
- **2026-07-08** — Phase 3 (Semantic Analysis) complete, incl. writing all 20 P0 benchmark `.nx` programs ahead of schedule. Commit `309f197`, pushed.
- **2026-07-08** — Phase 4 (Tree-Walking Interpreter) complete; all 20 benchmarks produce hand-verified correct output. Also fixed a sema bug (`arr.length` wrongly accepted as an assignment target) found while designing this phase. Not yet committed.
