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
| 2 — AST & Parser | ✅ Done | Yes | *(uncommitted)* |
| 3 — Semantic Analysis | ⬜ Not started | — | — |
| 4 — Tree-Walking Interpreter | ⬜ Not started | — | — |
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
- Not yet committed.

### Phase 3 — Semantic Analysis ⬜
Not started.

### Phase 4 — Tree-Walking Interpreter ⬜
Not started.

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
- **2026-07-08** — Phase 2 (AST & Parser) complete. Not yet committed.
