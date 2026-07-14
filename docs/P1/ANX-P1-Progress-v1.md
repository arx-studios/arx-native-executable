# ANX — P1 Progress Tracker (v1)

**Companion to:** [ANX-Progress-v1.md](../P0/ANX-Progress-v1.md) (P0, frozen), [ANX-P1-Operators-Plan-v1.md](ANX-P1-Operators-Plan-v1.md), [ANX-P1-Dogfooding-Notes-v1.md](ANX-P1-Dogfooding-Notes-v1.md)
**Scope:** P1 = Operators only. Every later slice (strings, classes, generics, collections, Tree/Graph, diagnostics, benchmark suite) has its own folder and tracker — see the repo's [docs index](../../README.md#docs) rather than this file for those.
**Purpose:** living record of what's actually been built for P1 — check this before assuming its state; it's the source of truth over memory of past conversations.

**How to update:** same discipline as the P0 tracker — when the exit gate is met, flip the status, append a changelog line with date and commit hash.

---

## Status: ✅ Done

All six planned steps implemented in order: lexer (17 new tokens), parser (4 new precedence levels + ternary/compound-assign parsing), sema, interpreter, codegen, dogfooding re-validation.

- New: ternary (`?:`), compound assignment (`+= -= *= /= %= &= |= ^= <<= >>=`), bitwise (`& | ^ ~`), shift (`<< >>`). Relational/equality operators were already present (P0) — the original ask's "rational" was treated as a likely typo for "relational" per the plan's own note, and confirmed nothing new was needed there.
- **Compound assignment is a real `Expr::CompoundAssign` AST node, not parser sugar for `target = target op value`** — per the plan's §1 decision, avoiding double-evaluating an `Index` target's array/index sub-expressions. Verified directly: both an interpreter test and a JIT-compiled codegen test confirm a side-effecting index expression (`arr[nextIndex()] += 5`) calls `nextIndex()` exactly once, on both paths.
- **One real design gap found and fixed while implementing, before it could surprise codegen**: sema originally allowed both ternary branches to type as `Void` (e.g. `cond ? print(1) : print(2)`) — but LLVM has no void-typed `phi` node. Added a sema check rejecting void ternary branches, matching how Java/C already disallow this.
- **Ternary codegen reused an existing pattern, cheaper than the plan anticipated**: the Operators Plan flagged `phi`-node codegen as "the one genuinely unfamiliar LLVM construct" — but P0's `&&`/`||` short-circuit codegen already used a `phi` node (for the boolean result), so ternary's only new wrinkle was generalizing from an always-`i1` phi to a phi typed per the ternary's actual result type (`Int`/`Float`/`Str`/`Array`, dispatched via sema's resolved-type side table).
- Right shift is arithmetic (sign-extending) per the plan's decision — verified via `~a` and `a >> b` JIT tests. Shift amounts use Rust's `wrapping_shl`/`wrapping_shr` in the interpreter specifically to avoid a Rust-level panic on out-of-range shift amounts (naive `<<`/`>>` panics in debug builds), while LLVM's `shl`/`ashr` are used as-is in codegen (already well-defined enough not to crash the *process*, even where the shift amount itself is unspecified).
- **A separate, pre-existing P0 gap surfaced while writing tests, unrelated to this phase's own scope**: codegen's `compile()` has no handling at all for top-level `Decl::Var` (global variables) — sema and the interpreter both support globals, but codegen silently ignores them. Not fixed here (out of scope for Operators); a codegen test that incidentally relied on a global was rewritten to use an array-based counter instead. Flagged here so it isn't lost — whichever later phase first needs a global should pick this up.
- 4 new lexer tests, 8 new parser tests (incl. 3 precedence-specific), 11 new sema tests, 6 new interpreter tests, 9 new codegen tests (incl. 5 JIT correctness checks) — 42 new tests total, all passing alongside the full existing P0 suite (196 tests total, 0 regressions).
- **Dogfooding re-validation (the plan's own Step 6 exit gate)**: re-solved "Single Number" (P0 Dogfooding Notes entry 9) using real `^=`-folding instead of the original O(n²) workaround — correct result (`4`) on both paths, confirming the friction found in Phase 8 is genuinely closed, not just theoretically addressed.
- **Exit gate** (per the Operators Plan's per-step gates, all met): lexer tokens correct including 3-char lookahead disambiguation (`<` vs `<<` vs `<<=`); precedence tree shapes verified; sema valid/invalid battery passing; interpreter correctness incl. the double-evaluation check; codegen verifiable IR + JIT correctness on ternary/bitwise/shift/compound-assignment; XOR re-solve of Single Number matches on both paths.

**Extended after initial completion**, against a full Java-operator-category checklist: added `++`/`--` (prefix *and* postfix, with correct differing return values — prefix returns the new value, postfix the old one) and `>>>`/`>>>=` (unsigned/logical right shift, zero-filling instead of sign-extending).
- `++`/`--` needed a dedicated `Expr::IncDec` AST node, not reuse of `CompoundAssign` — same "resolve the target's slot exactly once" concern for `Index` targets (`arr[f()]++` must call `f()` once), verified directly in interpreter and JIT codegen tests, plus one more check compound assignment didn't need: confirming *which* value (old vs. new) each form returns.
- `>>>` turned out cheap: the LLVM builder call already used for `>>` (`build_right_shift`) already took a `sign_extend: bool` parameter — `>>>` is just the same call with `false` instead of `true`, both in codegen and (via `(a as u64).wrapping_shr(...)`) in the interpreter.
- **`instanceof` deliberately not implemented** — flagged to the user rather than built hollow: it requires runtime type-checking against a class hierarchy with subtyping, but the P3 Classes Plan already decided interfaces/inheritance/virtual dispatch are dropped from the roadmap entirely (static dispatch only). Without subtyping, `instanceof` has no real question to answer in ANX — a variable's type is already known statically. Revisit only if a future decision reintroduces some form of subtyping.
- 3 new lexer tests, 6 new parser tests, 6 new sema tests, 5 new interpreter tests, 8 new codegen tests (incl. 6 JIT correctness checks) — 28 more tests, 224 total passing, 0 regressions.
- Commit: `224e144` (covers all of the above — original Operators work and this extension were committed together) — pushed.

---

## Changelog

- **2026-07-14** — Docs reorganized: `docs/P0/` (frozen) and `docs/P1/` (active) created.
- **2026-07-14** — Operators complete: ternary, compound assignment, bitwise, shift — full lexer→parser→sema→interp→codegen pipeline, 42 new tests, 196 total passing. Fixed a sema gap (void-typed ternary branches) found while designing codegen, and flagged a separate pre-existing P0 gap (codegen has no global-variable support) found incidentally while writing tests. Re-solved the P0 dogfooding "Single Number" friction point with real XOR — confirmed closed on both paths. Commit `224e144` (bundled with the extension below), pushed.
- **2026-07-14** — Docs restructured to a flat `P0`–`P8` scheme, one number per major slice. This tracker rewritten to cover P1/Operators only — see the repo docs index for everything else. Commit `0c2bad5`, pushed.
- **2026-07-14** — Operators extended against a full Java-operator checklist: added `++`/`--` (prefix + postfix, correct differing return values) and `>>>`/`>>>=` (unsigned/logical right shift). `instanceof` explicitly not implemented — flagged as requiring a class hierarchy with subtyping the roadmap has already decided not to build. 28 more tests, 224 total passing. Commit `224e144`, pushed.
