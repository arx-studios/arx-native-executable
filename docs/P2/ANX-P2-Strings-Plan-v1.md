# ANX — P2 Strings Implementation Plan (v1)

**Companion to:** [ANX-PRD-v1.md](../P0/ANX-PRD-v1.md), [ANX-Implementation-Plan-v1.md](../P0/ANX-Implementation-Plan-v1.md) (P0), [ANX-Syntax-Draft-v1.md](../P0/ANX-Syntax-Draft-v1.md) (P0), [ANX-Progress-v1.md](../P0/ANX-Progress-v1.md) (P0), [ANX-P1-Operators-Plan-v1.md](../P1/ANX-P1-Operators-Plan-v1.md) (P1), [ANX-P3-Classes-Plan-v1.md](../P3/ANX-P3-Classes-Plan-v1.md) (P3)

**Scope:** Real string support (length, indexing, concat, substring, equality) — the smallest self-contained feature that unblocks real DSA practice, sequenced right after Operators (P1) and before classes (P3). 2D arrays are the deliberately-deferred fast-follow (§6) once this ships, since dogfooding flagged both as blocking. **Numbering note:** see [P3's numbering note](../P3/ANX-P3-Classes-Plan-v1.md) — flat `P0`–`P8` slices, not the PRD's own milestone labels.

One planned follow-up once P3 (classes) ships and method-call parsing exists: give strings real method-call syntax (`s.length()`, `s.charAt(i)`) instead of the free-function syntax below, matching Java familiarity — the same kind of special-casing already used for `arr.length`, not a reason to make `String` a literal user-space class.

**Why strings, whenever this slice lands:** P0 scoped strings down to "literals, print-only" explicitly (Implementation Plan §1) to avoid scope creep before the pipeline worked end to end. That pipeline now exists (Phases 0–7 done, per Progress). Strings are the single highest-friction gap for real LeetCode-style practice (palindrome, anagram, reverse-string, valid-parentheses-on-chars, etc. are all string problems) and — critically — the interpreter path is nearly free, since `Value::Str` already wraps a Rust `String`. LLVM codegen is the real work.

---

## 1. Key design decisions

Resolving these up front, same pattern as the original Implementation Plan §1:

| Question | Decision | Rationale |
|---|---|---|
| Mutability | **Immutable**, Java-`String`-style. Every concat/substring produces a new backing buffer. | Matches the existing memory model exactly — "leak on exit," `malloc`/never-free (Implementation Plan §1). Immutable strings mean no aliasing hazards to reason about, no `StringBuilder` needed for P1. Mutation (`StringBuilder`-equivalent) stays parked for P2 if profiling ever shows string-building is a real bottleneck in practice problems — unlikely at DSA-input scale. |
| Runtime layout | **Mirror the array struct exactly**: `{ i64 length, ptr data }`, `data` heap-allocated via `malloc`, byte buffer, null-terminated (for free interop with the existing `anx_print_str` C shim, which expects a C string). | Reuses a design that's already proven in codegen (Phase 5) rather than inventing a second convention. Opaque pointers mean this composes with the array work for free later. |
| Indexing / `charAt` | **`charAt(s, i)` returns a length-1 string**, not a new `char` primitive type. | Adding `char` means a new lexer token, a new `Type` variant, new codegen lowering, and a decision about `char` vs `int` arithmetic — real scope for a "ship fast" pass. Returning a 1-length string keeps the type system at its current size (still just `int/float/bool/string/array`) and every comparison (`charAt(s,i) == "a"`) reads naturally. Revisit `char` only if dogfooding friction specifically demands it. |
| Method-call syntax (`s.length()`, `s.charAt(i)`) | **Not implemented now — use existing function-call syntax and reuse the existing `.length` field-access node for the property case.** `length(s)`, `charAt(s, i)`, `substring(s, start, end)` are plain function calls; `s.length` reuses `Expr::FieldAccess`, already implemented for `arr.length` (Progress, Phase 2). | Real `.method(args)` dispatch is method-call parsing that doesn't exist yet — it's rightly parked for when classes land (P1 proper, per PRD). Building a one-off version just for strings would be thrown away when classes add general method dispatch. Free-function syntax costs zero new grammar. |
| Concatenation operator | **Overload `+`** for `string + string`, mirroring Java. | Every DSA string-building loop reaches for `+`. Restricting to a `concat(a, b)` function would fight against exactly the syntax students already know from Java — the PRD's whole premise is not fighting familiar syntax. |
| Equality | **`==`/`!=` do content comparison** for strings, not reference comparison. | Reference equality on strings makes `s == "ab"` unreliable depending on whether literals are interned — a footgun with zero DSA upside. Content comparison is what every "is this a palindrome / anagram" problem actually needs. |
| Bounds checking | **Reuse the exact guard/panic pattern** already built for arrays in Phase 6 (`anx_panic_oob`-style), for `charAt` and `substring`. | Consistency the codebase already established: compiled and interpreted paths must fail identically (Usage Flow doc's stated bar). New panic fn: `anx_panic_str_oob`. |

**One thing this explicitly does NOT do:** touch the `Type` enum's variant count, the lexer's keyword set, or the parser's grammar. Every new capability above is either a builtin function or reuses `FieldAccess`/`Binary` nodes that already exist. That's what keeps this a fast P1 slice instead of a second frontend pass.

---

## 2. New builtin function signatures

Registered the same way `print()` presumably already is — pre-hoisted into sema's function-signature table before user code is walked, so ordinary call-checking (arg count, arg types, return type) just works with no special-casing at call sites:

```
int    length(string s)
string charAt(string s, int i)
string substring(string s, int start, int end)   // [start, end), like Java
```

`s.length` (field access, not a call) continues to work exactly like `arr.length` — same `Expr::FieldAccess` node, sema just needs to accept a `Str`-typed receiver in addition to `Array`.

`+` and `==`/`!=` are extended in the existing `BinOp` type-checking rules (sema/types.rs), not new syntax.

---

## 3. Phase-by-phase plan

Following the same "concrete exit gate per phase" discipline as the original Implementation Plan.

### Step 1 — Sema (`src/sema/types.rs`, `src/sema/mod.rs`)
- Pre-register the 3 builtin signatures above in the function-signature table before user-function hoisting.
- Extend `FieldAccess` type-checking: `Str` receiver + `"length"` → `Type::Int` (same code path as arrays, one new match arm).
- Extend `BinOp::Add` checking: `(Str, Str) → Str` alongside the existing numeric cases.
- Extend `BinOp::Eq`/`Neq` checking: allow `(Str, Str)` in addition to existing numeric/bool comparisons.
- **Exit gate:** a test program using `length()`, `charAt()`, `substring()`, `+`, and `==` on strings type-checks with zero errors; each wrong-arg-type/arg-count case on the three builtins produces the expected sema error, same as any other function call.

### Step 2 — Interpreter (`src/interp/mod.rs`)
- `length` → `s.chars().count()` (or `.len()` if byte-length is the intended DSA semantics — flagging this as the one open question worth a quick decision, see §5).
- `charAt(s, i)` → bounds-check, return a 1-char `String`; out-of-range → the existing `RuntimeError` pattern (same shape as array OOB).
- `substring(s, start, end)` → bounds-check `0 <= start <= end <= length`, slice.
- `+` on two `Value::Str` → Rust string concatenation (literally `format!("{a}{b}")`).
- `==`/`!=` on two `Value::Str` → Rust's own `PartialEq` on `String`, already correct.
- **Exit gate:** all of the above run correctly through `anx run` on a small hand-written test set (palindrome check, string reversal via `charAt` loop, anagram check via sorted-char comparison or count array). This step should be near-trivial given `Value::Str` already wraps Rust's `String`.

### Step 3 — LLVM Codegen (`src/codegen/mod.rs`, `runtime.rs`, `runtime.c`) — the real work
- Change string literal lowering from the current raw-`i8*`-global (print-only) to the `{ i64 length, ptr data }` struct, matching arrays. Literal buffers can still live as global constants; codegen wraps them in the struct at the point of use.
- New C runtime shim functions (`runtime.c`), following the exact pattern of the existing `anx_print_*` / array-guard functions:
  - `anx_str_length(AnxStr) -> i64`
  - `anx_str_concat(AnxStr, AnxStr) -> AnxStr` — `malloc`s a new buffer, `memcpy`s both, never frees inputs (consistent with the leak-on-exit model)
  - `anx_str_char_at(AnxStr, i64) -> AnxStr` — bounds-checked, returns a 1-length string struct
  - `anx_str_substring(AnxStr, i64, i64) -> AnxStr` — bounds-checked, `malloc` + `memcpy` slice
  - `anx_str_equals(AnxStr, AnxStr) -> i1` — length check + `memcmp`
  - `anx_panic_str_oob(...)` — same pattern as the existing `anx_panic_oob`/`anx_panic_div_zero`/`anx_panic_neg_size` trio from Phase 6
- Wire `length()`/`charAt()`/`substring()` calls, `s.length`, string `+`, and string `==`/`!=` in the AST to `call` instructions against these declared runtime functions — same lowering strategy already used for array bounds guards.
- Update the `print()` call path: since strings are now a struct, extract `.data` before passing to the existing `anx_print_str(i8*)` — one-line change at the print call site.
- **Exit gate:** every new construct emits verifiable IR (`Module::verify()`), matching the bar Phase 5 used originally. Then — going one step further, matching what Phase 5 actually did beyond its own exit gate — JIT-execute a few (concat, substring, equals) and assert correct results before ever touching the CLI/build path.

### Step 4 — CLI / Pipeline sanity (no new code expected)
- `anx run` and `anx build` already share the same frontend + codegen; no `cli.rs` changes anticipated. This step is just re-running the existing `anx build <file> -o <out> && ./out` check from Usage Flow on a string-using program to confirm the compiled path matches the interpreted one, same "both paths must behave identically" bar as everywhere else in this codebase.
- **Exit gate:** a hand-written test program (e.g., "is this string a palindrome") produces identical output on `anx run` and a built-then-executed binary.

### Step 5 — Dogfooding validation
- Add 2–3 new entries to `ANX-Dogfooding-Notes-v1.md`, deliberately string-shaped problems distinct from the benchmark suite and distinct from the already-logged N-Queens entry: e.g. valid anagram, reverse string in place (well — immutable, so "build the reversed string"), longest common prefix.
- **Exit gate:** solved without falling back to Java; friction logged the same way Phase 8 already does.

---

## 4. What's still explicitly out of scope after this ships

Carried forward, not forgotten:

- `char` as a distinct primitive type
- String mutation / `StringBuilder`
- Real method-call syntax (`s.charAt(i)` instead of `charAt(s, i)`) — waits for classes
- `split()`, `compareTo()`, regex, formatting — none of these came up as blocking in your answers; add only if a specific dogfood problem needs one
- Unicode correctness beyond whatever `length()`'s byte-vs-char decision settles (see §5) — not a concern at DSA-practice scale

---

## 5. One open question worth answering before Step 2

**Does `length()` count bytes or Unicode scalar values?** For plain ASCII DSA inputs (the overwhelming majority of practice problems) these are identical, so this is genuinely low-stakes — but Rust's `.len()` (bytes) and `.chars().count()` (scalars) diverge the moment a non-ASCII character shows up, and LLVM-side `strlen`-equivalent logic only gives you bytes for free. Recommendation: **byte length**, matching what the LLVM runtime shim can compute cheaply via `strlen`/stored length with no per-call scan — and it's what Java's `.length()` effectively does too (UTF-16 code units, not "characters" in the Unicode sense, so Java isn't actually the more "correct" choice here anyway).

---

## 6. Fast-follow: 2D arrays (next up, not started)

Flagging scope now since you called this out as equally blocking, so Step 3 above can be designed with half an eye on this:

- The `Implementation Plan`'s original scoping (§1) deliberately chose flattened-1D-with-manual-indexing over real `int[][]` to keep P0 minimal — that decision was correct for P0, but the `Type` enum already has `Array(Box<Type>)` (Tech Stack doc, Phase 3 row), meaning `Array(Array(Int))` is likely already representable in sema's type system with zero enum changes.
- What's actually missing: (1) parser support for `new int[rows][cols]` nested allocation syntax, (2) codegen for a **jagged** array-of-array-structs (true Java semantics — each row independently `malloc`'d, not a flattened matrix), (3) nested array-literal syntax `[[1,2],[3,4]]` if you want that too, not just `new`-allocation.
- This reuses the exact `{i64 length, ptr data}` struct pattern this strings plan establishes, just with `data` pointing at an array of structs instead of an array of scalars/bytes — meaning shipping strings first (this doc) directly de-risks the 2D-array work, since the struct-composition pattern gets proven out here first.
- Will write this up as its own companion doc (`ANX-P2-2DArrays-Plan-v1.md`, alongside this file in `docs/P2/`) once strings are actually done — sequencing per your answer.
