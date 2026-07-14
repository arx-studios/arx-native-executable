# ANX — P1 Tech Stack (v1)

**Companion to:** [ANX-Tech-Stack-v1.md](../P0/ANX-Tech-Stack-v1.md) (P0), [ANX-P1-Operators-Plan-v1.md](ANX-P1-Operators-Plan-v1.md)
**Scope:** P1 = Operators only. Other slices' tech notes now live in their own folders ([P2](../P2/ANX-P2-Strings-Plan-v1.md), [P3](../P3/ANX-P3-Classes-Plan-v1.md), [P4](../P4/ANX-P4-Generics-Plan-v1.md), [P5](../P5/ANX-P5-Collections-Plan-v1.md)) rather than one shared post-P0 doc.
**Purpose:** what's actually new in the stack for Operators vs. what's just extending P0's existing tools.

## Headline: no new dependencies

Operators is built entirely on the stack the [P0 Tech Stack doc](../P0/ANX-Tech-Stack-v1.md) already established — same Rust toolchain, same `inkwell`/LLVM 21, same `clap`/`thiserror`/`anyhow`, same `assert_cmd`/`predicates` for testing. `Cargo.toml` didn't change at all for this work.

## What's genuinely new (technique, not tooling)

LLVM `phi` nodes for ternary codegen (Operators Plan, Step 5). P0 never needed one — `if`/`else` was statement-only, so control flow never had to "merge a value" from two branches. In practice this turned out cheaper than expected: P0's `&&`/`||` short-circuit codegen already used a `phi` node for its boolean result, so ternary only generalized an existing pattern rather than introducing a wholly new one.

## Testing — unchanged

`cargo test` remains the whole story: unit tests per layer (same `#[cfg(test)] mod tests` pattern throughout `src/`), CLI integration tests via `assert_cmd`/`predicates` (`tests/cli.rs`), and the dual-path (interpreter + compiled) benchmark suite (`tests/integration.rs`).
