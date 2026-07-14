# ANX — P7 Diagnostics Polish Plan (v1)

**Companion to:** [ANX-Implementation-Plan-v1.md](../P0/ANX-Implementation-Plan-v1.md) (P0)

**Scope:** clearer compiler error messages with line *and* column numbers. Independent of the class-system slices (P3–P6) — could in principle be done any time, sequenced here mostly because it's low-risk mechanical work well-suited to slot in once the higher-value features are shipped.

**Numbering note:** see [P3's numbering note](../P3/ANX-P3-Classes-Plan-v1.md).

---

## Phase plan

- **Concrete, verified gap:** `Token` already carries both `line` *and* `col` (confirmed directly in `src/lexer/token.rs`), but `ParseError` and `SemaError` currently only propagate `line` — `col` is silently dropped between the lexer and every downstream error. Threading it through is a mechanical plumbing fix, not new infrastructure, exactly as the original P0 plan anticipated ("the line/column info already threaded through since Phase 1 makes this mostly a message-quality pass").
- Beyond column numbers: audit actual error message text for clarity (e.g. is `"expected Int, found Identifier("x")"`-shaped `Debug`-derived output good enough, or does it need hand-written `Display` phrasing per token kind?) and consider a "did you mean `<similar name>`?" suggestion for undeclared-identifier errors, which is cheap (edit-distance over already-known symbol names) and disproportionately useful for a learning-focused language.
- **Exit gate:** every error variant reports `line:col`; a manual pass confirms message wording reads clearly for a CS student, not just a compiler author.
