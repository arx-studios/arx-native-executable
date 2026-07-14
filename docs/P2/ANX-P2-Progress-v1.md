# ANX — P2 Progress Tracker (v1)

**Companion to:** [ANX-P2-Strings-Plan-v1.md](ANX-P2-Strings-Plan-v1.md), [ANX-P2-Dogfooding-Notes-v1.md](ANX-P2-Dogfooding-Notes-v1.md), [../P1/ANX-P1-Progress-v1.md](../P1/ANX-P1-Progress-v1.md) (P1 tracker)
**Scope:** P2 = Strings only (`length`, `charAt`, `substring`, `s.length`, `+` concat, `==`/`!=` content comparison). Every later slice (classes, generics, collections, Tree/Graph, diagnostics, benchmark suite) has its own folder and tracker — see the repo's docs index rather than this file for those.
**Purpose:** living record of what's actually been built for P2, step by step, against the Strings plan's exit gates — check this before assuming its state; it's the source of truth over memory of past conversations.

**How to update:** when a step's exit gate is met, flip its row in the table below, fill in its detail section, and append one line to the changelog at the bottom with the date and commit hash.

---

## Status at a glance

| Step | Status | Exit gate met? | Commit(s) |
|---|---|---|---|
| 1 — Sema | ✅ Done | Yes | `a82472e` |
| 2 — Interpreter | ✅ Done | Yes | `a82472e` |
| 3 — LLVM Codegen | ✅ Done | Yes | `a82472e` |
| 4 — CLI / Pipeline sanity | ✅ Done | Yes | `a82472e` |
| 5 — Dogfooding validation | ✅ Done | Yes | `a82472e` |

All five steps landed together in a single commit rather than one per step — unlike P0/P1, this slice was small enough to implement and verify start-to-finish in one sitting.

---

## Step details

### Step 1 — Sema ✅
- The 3 builtin signatures (`int length(string)`, `string charAt(string, int)`, `string substring(string, int, int)`) are pre-registered in the symbol table (`Checker::declare_builtin_funcs`, `src/sema/mod.rs`) before user-function hoisting runs, so they resolve through the ordinary `check_call` path — arity checking, arg-type checking, everything — with zero special-casing at call sites. A side effect worth noting: a user function named `length` correctly gets rejected as a duplicate declaration, since the builtin is already registered by the time hoisting's own duplicate check runs.
- `FieldAccess` type-checking (`src/sema/types.rs`) extended with one new match arm: `(Type::Str, "length") => Some(Type::Int)`, alongside the existing array case.
- `BinOp::Add` extended for `(Str, Str) → Str`. `Eq`/`NotEq` needed **no code change at all** — the existing generic rule (`lt == rt → Some(Type::Bool)`) already covers `Str == Str` correctly, since `Type` derives `PartialEq`.
- 8 new sema tests: valid usage of all 3 builtins + `+`/`==`/`!=` together, `s.length` field access, and one error test per builtin's arity/arg-type mismatch plus `Str + Int` and `Str == Int` mismatches.
- **Exit gate** ("a test program using `length()`, `charAt()`, `substring()`, `+`, and `==` on strings type-checks with zero errors; each wrong-arg-type/arg-count case on the three builtins produces the expected sema error"): met.
- Commit: `a82472e` (pushed to `origin/main`, alongside all other P2 steps).

### Step 2 — Interpreter ✅
- Near-trivial, as the plan predicted (§ "Why strings, whenever this slice lands") — `Value::Str` already wraps a Rust `String`, so `+` is `format!`/string concatenation and `==`/`!=` ride Rust's own derived `PartialEq` unchanged.
- `length()` counts **bytes** (`.len()`), not Unicode scalars, per the plan's §5 recommendation — cheap, matches the LLVM runtime shim's `strlen`-equivalent cost, and matches what Java's `.length()` effectively does anyway (UTF-16 code units, not Unicode characters).
- `charAt`/`substring` are bounds-checked against byte length; a new `RuntimeError::StringIndexOutOfBounds { index, length }` variant (same shape as the existing array one, distinct message text) covers violations, returned as `Err` — never a Rust panic, matching the established runtime-error discipline.
- 7 new interpreter tests: builtin/operator correctness, both bounds-error cases, and — going beyond unit-level checks, matching Phase 4's own precedent — 3 real DSA-shaped programs run end-to-end (palindrome check via `charAt` loop, string reversal via `+` accumulation, valid anagram via 26-letter counting).
- **Exit gate** ("all of the above run correctly through `anx run` on a small hand-written test set — palindrome check, string reversal via `charAt` loop, anagram check"): met.
- Commit: `a82472e` (pushed to `origin/main`, alongside all other P2 steps).

### Step 3 — LLVM Codegen ✅
- **The real work, per the plan's own framing.** String literals changed from a print-only raw `i8*` global to the `{ i64 length, ptr data }` struct arrays already use — `data` is a null-terminated global byte buffer (`build_global_string_ptr` already null-terminates), so `anx_print_str` needed no separate lowering path for strings.
- The shared struct-type/struct-builder helpers were renamed to reflect they're no longer array-only: `array_struct_type` → `len_data_struct_type`, `build_array_struct` → `build_len_data_struct` (`src/codegen/mod.rs`).
- Because both `Type::Str` and `Type::Array(_)` are now struct-valued in LLVM, two call sites that previously dispatched on the raw LLVM value's shape had to switch to dispatching on sema's resolved type instead, since the two are no longer distinguishable by shape alone: `codegen_print` (previously "a `PointerValue` argument means print a string") and the ternary `phi` codegen (whose `Str`/`Array` match arms are now identical and were merged).
- New C runtime shim functions in `src/codegen/runtime.c` (declared in `runtime.rs`): `anx_str_concat`, `anx_str_char_at`, `anx_str_substring`, `anx_str_equals`, `anx_panic_str_oob`. Struct-by-value params/returns across this C boundary rely on the same by-value struct ABI lowering already proven correct for array parameters passed between ANX functions — no new ABI risk introduced.
- `charAt`/`substring` bounds-checking is **inline in codegen** (`emit_runtime_guard` + `anx_panic_str_oob`), mirroring the array index check almost line-for-line per the plan's explicit instruction — the C shim's malloc+memcpy work only runs once codegen has already confirmed the indices are valid; the shim itself does not re-validate.
- **One deliberate deviation from the plan's shim list, flagged rather than silently applied:** `anx_str_length` was *not* implemented. `length(s)` and `s.length` both compile to a direct `extractvalue` on the struct's field 0 — no runtime call at all — exactly matching how array `.length` already worked before this phase. A C function that would do nothing but return a value codegen already has in a register is pure overhead with no upside; revisit only if a future refactor changes the string layout such that length needs real computation (e.g. lazy or UTF-8-aware length).
- `anx_str_equals` crosses the C ABI boundary as `i8` (0/1), not `i1` — matches the existing `anx_print_bool` convention for booleans at this boundary; codegen truncates the `i8` result back to `i1` at the one call site.
- JIT correctness tests for `charAt`/`substring`/`concat`/`equals` needed real (not panic-stub) Rust implementations of the C shim mapped into the execution engine — unlike the pre-existing panic stubs, which only need to exist for an *untaken* failure path, these tests exercise the happy path and need actually-correct results. Added `#[repr(C)]` Rust equivalents bit-for-bit matching the `{i64, ptr}` layout.
- 2 new `Module::verify()`-only tests (full builtin/operator/field-access surface, plus ternary/compound-assign on strings) and 5 new JIT-execution correctness tests (concat+length+equality, `charAt`, `substring` incl. an empty-slice edge case, `s.length` field access, `!=`).
- **Exit gate** ("every new construct emits verifiable IR; JIT-execute concat/substring/equals and assert correct results"): met.
- Commit: `a82472e` (pushed to `origin/main`, alongside all other P2 steps).

### Step 4 — CLI / Pipeline sanity ✅
- As anticipated, no `cli.rs` changes were needed — `anx run` and `anx build` already share the same frontend + codegen.
- Hand-written test programs confirmed byte-identical output between `anx run` and a built-then-executed binary for a palindrome-check + string-reversal program (`isPalindrome`, `reverseString`, plus `.length`, `substring`, `+`, `==`/`!=` all exercised in one program).
- Also went one step further than the plan's own gate: a deliberate `charAt` out-of-bounds case was run on both paths and produced the **exact same** `runtime error: string index 5 out of bounds for length 2` message and exit code `2` on both — confirming the new bounds-guard codegen actually fails identically to the interpreter, not just that the happy path matches.
- **Exit gate** ("a hand-written test program produces identical output on `anx run` and a built-then-executed binary"): met.
- Commit: `a82472e` (pushed to `origin/main`, alongside all other P2 steps).

### Step 5 — Dogfooding validation ✅
- All 3 plan-suggested problems solved and verified on both paths with zero friction: valid anagram (26-letter counting via `charAt`/`==`), longest common prefix (`substring` for the actual extraction, including the empty-prefix edge case), and build-the-reversed-string (immutable-string accumulation via `+`, including the empty-string edge case).
- Logged in [ANX-P2-Dogfooding-Notes-v1.md](ANX-P2-Dogfooding-Notes-v1.md), same honesty-note discipline as the P0/P1 logs (Claude-solved, verifying language capability — not Ayushman's own practice).
- **Exit gate** ("solved without falling back to Java; friction logged the same way Phase 8 already does"): met — no friction to log; immutability cost nothing at DSA-practice string lengths.
- Commit: `a82472e` (pushed to `origin/main`, alongside all other P2 steps).

---

## Changelog

- **2026-07-14** — Strings (P2) implemented end-to-end, all 5 steps in one pass: sema (builtin pre-registration, `FieldAccess`/`BinOp` extensions) → interpreter (byte-length semantics, bounds-checked `charAt`/`substring`, 3 DSA-shaped correctness tests) → codegen (string literals moved to the shared `{i64, ptr}` struct layout, new `anx_str_*` C runtime shim, inline bounds guards mirroring array indexing, 5 JIT correctness tests) → CLI sanity (both paths verified byte-identical on output *and* on a deliberate runtime-error case) → dogfooding (3/3 plan-suggested problems solved, zero friction). Deliberately skipped `anx_str_length` as unnecessary (direct struct-field extraction already covers it, matching the pre-existing array `.length` precedent) — flagged rather than silently omitted. 22 new tests (8 sema, 7 interpreter, 7 codegen), 246 total passing, 0 regressions. Commit `a82472e`, pushed.
