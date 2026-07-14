# ANX — P5 Collections Implementation Plan (v1)

**Companion to:** [ANX-P3-Classes-Plan-v1.md](../P3/ANX-P3-Classes-Plan-v1.md) (P3 — shared key decisions §1), [ANX-P4-Generics-Plan-v1.md](../P4/ANX-P4-Generics-Plan-v1.md) (P4), [ANX-P6-TreeGraph-Plan-v1.md](../P6/ANX-P6-TreeGraph-Plan-v1.md)

**Scope:** `List`, `Stack`, `Queue`, `HashMap` — the standard-library prelude. Depends on P3 (classes) and P4 (generics) both shipping first.

**Numbering note:** see [P3's numbering note](../P3/ANX-P3-Classes-Plan-v1.md).

---

## Key decisions

Already made in [P3 §1](../P3/ANX-P3-Classes-Plan-v1.md#1-key-engineering-decisions): collections are written in ANX itself (not compiler intrinsics), bundled as an auto-included prelude with no import syntax. This doc just executes it.

## Phase plan

- Each collection written as an ordinary ANX generic class, using arrays (`new T[n]`, doubling on resize) for underlying storage — `List<T>`/`Stack<T>`/`Queue<T>` are straightforward; `HashMap<K, V>` needs a hash function. Scope: a simple hash (e.g. multiply-and-mod for `int` keys, a basic string hash for `string` keys) with linear-probing or chained buckets — collision-resistance/performance isn't a stated goal at DSA-practice scale.
- Bundled the same way `runtime.c` is today: an ANX-source string embedded in the `anx` binary via `include_str!`, lexed/parsed/sema'd/codegen'd alongside the user's own program at `anx run`/`anx build` time (prelude declarations processed before the user's own, so user code can reference them immediately — no import statement, per P3 §1).
- **Exit gate:** `Stack<int>`, `Queue<int>`, and `HashMap<string, int>` each pass a small correctness test (push/pop order, enqueue/dequeue order, put/get/overwrite) on both interpreter and compiled paths.

## Risks

| Risk | Mitigation |
|---|---|
| Prelude-as-ANX-source approach needs a language feature that doesn't exist yet (e.g., an assert/panic mechanism inside `HashMap.get` for a missing key) | P0's `RuntimeError` pattern (interpreter) / panic-guard pattern (codegen) already establishes how to fail cleanly at runtime — extend the same mechanism rather than inventing exceptions, since `try`/`catch` remains explicitly out of scope per the syntax draft. |
