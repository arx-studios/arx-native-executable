# ANX — Tech Stack (v1)

**Companion to:** [ANX-PRD-v1.md](ANX-PRD-v1.md), [ANX-Syntax-Draft-v1.md](ANX-Syntax-Draft-v1.md), [ANX-Implementation-Plan-v1.md](ANX-Implementation-Plan-v1.md), [ANX-Usage-Flow-v1.md](ANX-Usage-Flow-v1.md)
**Purpose:** exactly what tool/crate/library is used at each step of the Implementation Plan, and why — so there's no ambiguity when a phase starts.

---

## At a glance

| Layer | Choice |
|---|---|
| Host language (the compiler's own implementation) | Rust, stable channel, 2021 edition |
| Build system / package manager | Cargo |
| CLI parsing | `clap` v4 (derive API) |
| Error handling | `thiserror` (typed errors within the compiler) + `anyhow` (CLI-boundary error reporting) |
| Lexer | Hand-written, `std` only — no lexer-generator crate |
| Parser | Hand-written recursive descent + Pratt (precedence climbing), `std` only — no parser-generator crate |
| Semantic analysis | Hand-written, `std::collections::HashMap` for symbol tables — no crate |
| Interpreter | Hand-written tree-walker, `std::rc::Rc` + `std::cell::RefCell` for array values — no crate |
| Native codegen | `inkwell` (safe-ish wrapper over `llvm-sys`), feature `llvm21-1` |
| LLVM itself | LLVM 21, installed via Homebrew (`llvm@21`), linked at build time |
| Runtime shim (print builtins) | Small `.c` file, compiled with the system C compiler |
| Object file → executable linking | System linker, invoked via `cc` as the linker driver (same as Rust/Clang do) |
| Process orchestration (shelling out to `cc`) | `std::process::Command` — no crate |
| Testing (unit) | Built-in `cargo test` / `#[test]` |
| Testing (CLI/integration) | `assert_cmd` v2 + `predicates` v3 |
| Version control | Git |
| CI | None in v1 (see §Explicitly not used) |

---

## Toolchain prerequisites (installed once, not per-phase)

```
# Rust
curl https://sh.rustup.rs -sSf | sh
rustup default stable

# LLVM 21
brew install llvm@21
export LLVM_SYS_211_PREFIX=$(brew --prefix llvm@21)

# System C compiler / linker (Xcode Command Line Tools, macOS)
xcode-select --install
```

These four (`rustup`, Cargo, `llvm@21`, Xcode CLT) are the entire toolchain footprint. Nothing else needs installing to build or use `anx`.

---

## Stack by phase

Mirrors the phases in [ANX-Implementation-Plan-v1.md §3](ANX-Implementation-Plan-v1.md).

### Phase 0 — Bootstrap
| Step | Tech | Notes |
|---|---|---|
| Project scaffold | `cargo new anx` | Standard binary crate, 2021 edition. |
| CLI arg parsing | `clap` v4, `derive` feature | Backs the `run`/`build`/`check` subcommands (Phase 6), added now so `main.rs` compiles from day one. |
| Error handling | `thiserror` for library-internal error enums (lexer/parser/sema errors), `anyhow` at the `main.rs`/CLI boundary to unify them for reporting | Standard pairing — `thiserror` gives typed errors per module, `anyhow` avoids needing a giant top-level enum just to print one and exit. |
| Test harness | Built-in `cargo test` | No test crate needed yet — just confirming the skeleton compiles and an empty test passes. |

### Phase 1 — Lexer
| Step | Tech | Notes |
|---|---|---|
| Tokenizing | Hand-written scanner over `&str`/`Vec<char>`, `std` only | No `logos` or similar lexer-generator crate — the token set is small and fixed (per the syntax draft grammar), hand-writing it is less machinery than wiring up a generator for a one-time grammar. |
| Errors | `thiserror`-derived `LexError` enum | Carries line/column per the plan's decision to attach position info from Phase 1 onward. |
| Tests | `#[test]` in `lexer/mod.rs` | Token-category tests + the binary-search worked example. |

### Phase 2 — AST & Parser
| Step | Tech | Notes |
|---|---|---|
| AST types | Plain Rust `enum`/`struct` definitions in `ast/mod.rs` | No AST-generation macro crate — the node set is small and stable enough that hand-defining it is simpler than a DSL for generating it. |
| Parsing | Hand-written recursive descent (statements/declarations) + Pratt/precedence-climbing (expressions), `std` only | No `pest`/`lalrpop`/`nom` — a hand-written parser keeps full control over error messages and avoids learning/maintaining a parser-generator's own grammar DSL for a language this size. |
| Errors | `thiserror`-derived `ParseError` enum | Same pattern as the lexer. |
| Tests | `#[test]` in `parser/mod.rs` | AST-shape assertions per construct. |

### Phase 3 — Semantic Analysis
| Step | Tech | Notes |
|---|---|---|
| Symbol table | `std::collections::HashMap` + `Vec` as a scope stack | No crate needed — this is plain data-structure work, and part of the point of the project is not reaching for a library where a `HashMap` suffices. |
| Type representation | Plain Rust `enum Type { Int, Float, Bool, Str, Array(Box<Type>), Void }` | No type-system crate. |
| AST annotation | `HashMap<NodeId, ResolvedType>` / `HashMap<NodeId, SymbolId>` side tables, `std` only | Concrete mechanism for the "one AST, shared sema" decision — no in-place mutation of node structs required. |
| Errors | `thiserror`-derived `SemaError` enum, collected into a `Vec<SemaError>` | Per the plan's "collect all errors, don't stop at first" requirement. |
| Tests | `#[test]` in `sema/mod.rs` | Valid/invalid program battery. |

### Phase 4 — Tree-Walking Interpreter
| Step | Tech | Notes |
|---|---|---|
| Runtime values | Plain Rust `enum Value { Int(i64), Float(f64), Bool(bool), Str(String), Array(Rc<RefCell<Vec<Value>>>), Void }` | `Rc<RefCell<_>>` gives arrays reference semantics (needed since ANX arrays are mutable and passed by reference, matching Java-like semantics) without a GC. |
| Scoping | `Environment` struct, parent-linked via `Rc<RefCell<Environment>>` | Chained scopes mirroring the sema symbol table exactly. |
| Execution | Direct AST walk (`std` only, no bytecode/VM layer) | A tree-walker is explicitly the design — no compiling to an intermediate bytecode for the interpreted path. |
| `print()` builtin | Rust's own `println!`/`print!` macros | The interpreter's `print` is just a native Rust function call — it's the *compiled* path (Phase 5) that needs a real C runtime shim, since the interpreter already runs inside a process with normal stdio. |
| Tests | `#[test]` in `interp/mod.rs`, plus running actual benchmark `.nx` files once they exist (Phase 7) | |

### Phase 5 — LLVM Codegen
| Step | Tech | Notes |
|---|---|---|
| LLVM IR emission | `inkwell` (`Context`, `Module`, `Builder`), feature `llvm21-1` | Chosen over raw `llvm-sys` for its safer, more ergonomic API around LLVM's C API; chosen over hand-emitting `.ll` text for type/API safety at build time. See [ANX-Implementation-Plan-v1.md §1](ANX-Implementation-Plan-v1.md) for the version rationale. |
| LLVM itself | System-installed LLVM 21 (via `llvm@21` Homebrew formula), linked against by `inkwell`/`llvm-sys` at `anx` build time | `LLVM_SYS_211_PREFIX` env var tells `llvm-sys` where to find it — no LLVM source vendored into this repo. |
| Array runtime layout | Hand-defined LLVM struct type `{ i64, T* }`, backed by `malloc` calls emitted directly as LLVM IR (`inkwell`'s `build_call` to the `malloc` intrinsic/declaration) | No custom allocator crate — "leak on exit" per the memory-model decision means there's nothing to manage beyond `malloc`. |
| `print()` builtin (compiled path) | Tiny bundled C runtile shim (`runtime.c`: `anx_print_int`, `anx_print_bool`, `anx_print_str`, `anx_print_array`), compiled with the system C compiler | Simplest way to get real `printf`-backed stdout from a native binary without hand-building `printf` format-string IR calls for every value type. |
| IR verification | `inkwell`'s `Module::verify()` | Gate before ever attempting to emit an object file. |
| Tests | `#[test]` in `codegen/mod.rs`, asserting `verify()` passes per construct | Running the actual compiled binary is Phase 6/7's job, not Phase 5's. |

### Phase 6 — Compile Pipeline & CLI
| Step | Tech | Notes |
|---|---|---|
| CLI subcommands (`run`/`build`/`check`) | `clap` v4 derive, defined in `cli.rs` | Same crate introduced in Phase 0. |
| Object file emission | `inkwell::targets::TargetMachine` (`write_to_file` with `FileType::Object`) | Standard LLVM target-machine API, no separate `llc` shell-out needed — `inkwell` wraps this directly. |
| Linking object file + runtime shim → executable | System `cc`, invoked via `std::process::Command` (e.g. `cc user.o runtime.o -o solve`) | Reuses the platform's existing linker driver rather than reimplementing linking — same approach `rustc` itself takes. No `build.rs` needed since this happens at `anx build` runtime, not at `anx`'s own compile time. |
| Tests | `#[test]` invoking the CLI binary directly (see Phase 7's `assert_cmd` usage) | |

### Phase 7 — Benchmark Suite
| Step | Tech | Notes |
|---|---|---|
| Test fixtures | Plain `.nx` + `.expected` file pairs under `tests/benchmarks/` | No test-data crate — just files, per [ANX-Implementation-Plan-v1.md §Phase 7](ANX-Implementation-Plan-v1.md). |
| Driving the `anx` binary from tests | `assert_cmd` v2 | Purpose-built for testing CLI binaries from within `cargo test` — handles locating/running the just-built `anx` binary and capturing stdout/exit code cleanly, instead of hand-rolling `std::process::Command` + manual output capture for every test case. |
| Assertions on output | `predicates` v3 (paired with `assert_cmd`) | Readable stdout/exit-code matchers (`predicate::str::diff(...)` etc.) instead of raw `assert_eq!` on captured bytes. |
| Test runner | `tests/integration.rs` under `cargo test`'s standard integration-test discovery | No custom test harness/runner crate needed — Cargo's built-in integration test mechanism is sufficient for 20 fixed cases. |

### Phase 8 — Dogfooding
No new tech — this phase is *using* the Phase 0–7 stack (`anx run` / `anx build`) to solve real problems, not building anything further.

---

## Explicitly not used (and why)

| Not used | Reason |
|---|---|
| Parser-generator crates (`pest`, `lalrpop`, `nom`) | Hand-written recursive descent gives full control over error messages/line info for a grammar this size; a generator adds a second DSL to maintain for no real benefit at this scale. |
| Lexer-generator crates (`logos`) | Same reasoning — small, fixed token set. |
| A bytecode VM for the interpreted path | The PRD explicitly calls for a tree-walking interpreter, not a compiled-bytecode VM; adding one would be unrequested scope. |
| A GC crate / `Arc`+ref-counted cycle collector | Memory-model decision (§1 of the Implementation Plan) is "leak on exit" — no GC needed for v1. |
| `build.rs` for compiling the runtime shim ahead of time | The shim is compiled fresh at each `anx build` invocation via a plain `Command::new("cc")` call — simpler than embedding a precompiled object into the `anx` binary itself for a project this size. |
| CI (GitHub Actions, etc.) | No external deadline or collaborators per the PRD's timeline section; `cargo test` run locally is the gate. Can be added trivially later if that changes. |
| Any LLVM version other than 21 | Confirmed `inkwell` supports 11–21; no reason to pin to an older release. |
