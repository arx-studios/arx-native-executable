# ANX — P2 Tech Stack (v1)

**Companion to:** [ANX-Tech-Stack-v1.md](../P0/ANX-Tech-Stack-v1.md) (P0), [ANX-P2-Strings-Plan-v1.md](ANX-P2-Strings-Plan-v1.md)
**Scope:** P2 = Strings only. Other slices' tech notes live in their own folders ([P3](../P3/ANX-P3-Classes-Plan-v1.md), [P4](../P4/ANX-P4-Generics-Plan-v1.md), [P5](../P5/ANX-P5-Collections-Plan-v1.md)) rather than one shared post-P0 doc.
**Purpose:** what's actually new in the stack for Strings vs. what's just extending P0's existing tools.

## Headline: no new dependencies

Strings is built entirely on the stack the [P0 Tech Stack doc](../P0/ANX-Tech-Stack-v1.md) already established — same Rust toolchain, same `inkwell`/LLVM 21, same `clap`/`thiserror`/`anyhow`, same `assert_cmd`/`predicates` for testing. `Cargo.toml` didn't change at all for this work.

## What's genuinely new (technique, not tooling)

- **`runtime.c` grows its first non-print, non-panic functions**: `anx_str_concat`, `anx_str_char_at`, `anx_str_substring`, `anx_str_equals` do real `malloc`+`memcpy` work and **return** a struct by value, not just take scalar arguments like every existing shim function (`anx_print_*`, `anx_panic_*`). This is the first time this codebase relies on LLVM's by-value struct *return* ABI lowering across the C boundary — struct *parameters* were already proven (arrays passed to ANX functions), but a struct-returning C function is new ground here, resolved by trusting the same System V classification LLVM/inkwell already use consistently for both directions.
- **`anx_panic_str_oob`** follows the exact `anx_panic_oob`/`anx_panic_div_zero`/`anx_panic_neg_size` shape from P0 Phase 6 — no new pattern, just one more instance of it.
- **JIT-testing a struct-returning C shim required real Rust stand-ins, not just panic stubs.** Every previous JIT correctness test (P0 Phase 5, P1 Operators) only needed the three panic functions mapped as stubs, since happy-path tests never call them. Strings' JIT tests exercise `anx_str_concat`/`char_at`/`substring`/`equals` on the happy path, so this phase adds `#[repr(C)]` Rust functions that actually replicate the C shim's logic (same `malloc`+`memcpy`, mapped into the execution engine via `add_global_mapping`, same as the stubs) — a genuinely new testing technique, not new tooling (still plain `cargo test`, no new test crate).

## Testing — unchanged tooling, one new technique

`cargo test` remains the whole story: unit tests per layer (same `#[cfg(test)] mod tests` pattern throughout `src/`), CLI integration tests via `assert_cmd`/`predicates` (`tests/cli.rs`), and the dual-path (interpreter + compiled) benchmark suite (`tests/integration.rs`). The one addition is the real-implementation JIT stand-ins noted above — no new crate, just a new pattern within the existing harness.
