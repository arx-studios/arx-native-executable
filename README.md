# ANX

*ANX: Arx Native eXecutable*

A small, real compiled language purpose-built for practicing data structures and algorithms — Java-like syntax, instant tree-walking interpretation for fast iteration, and genuine LLVM-backed native compilation so it's never just a toy.

## Why

CS students practicing DSA usually reach for Python, Java, or C++ — none of which were designed for the job. Python hides memory/runtime behavior behind abstraction; Java and C++ demand heavy boilerplate for the exact structures (stacks, queues, trees, graphs) DSA problems use constantly. ANX is an attempt at a language purpose-built for this: real compiled semantics, DSA-shaped built-ins, no setup ceremony. Full rationale in the [PRD](docs/P0/ANX-PRD-v1.md).

## Status

**P0 is complete** — lexer through native codegen, all 20 benchmark problems passing on both the interpreter and the compiled path. Currently in Phase 8: dogfooding (real DSA practice, tracking friction for P1). [docs/P0/ANX-Progress-v1.md](docs/P0/ANX-Progress-v1.md) is the authoritative, continuously-updated source of truth for what's actually built — treat anything below as a snapshot, not a promise.

| Component | Status |
|---|---|
| Lexer, parser, AST | ✅ |
| Semantic analysis (shared by both backends) | ✅ |
| Tree-walking interpreter | ✅ |
| LLVM 21 native codegen | ✅ |
| Compile pipeline + CLI (`anx check/run/build`) | ✅ |
| 20-problem benchmark suite, both paths | ✅ 20/20 |
| Dogfooding (10 real DSA problems) | 🟡 1/10 |

## Example

```
int binarySearch(int[] arr, int target) {
    int lo = 0;
    int hi = arr.length - 1;
    while (lo <= hi) {
        int mid = lo + (hi - lo) / 2;
        if (arr[mid] == target) return mid;
        else if (arr[mid] < target) lo = mid + 1;
        else hi = mid - 1;
    }
    return -1;
}

void main() {
    int[] nums = [1, 3, 5, 7, 9, 11];
    print(binarySearch(nums, 7));
}
```

## Getting started

**Prerequisites:** Rust (stable, via [rustup](https://rustup.rs)), LLVM 21 (`brew install llvm@21` on macOS), and a system C compiler (Xcode Command Line Tools on macOS) to link the runtime shim.

```bash
git clone <this repo>
cd anx
export LLVM_SYS_211_PREFIX=$(brew --prefix llvm@21)
cargo build --release
```

Put the binary on your `PATH` — permanently (add to `~/.zshrc`) or for the current session only:
```bash
export PATH="$PWD/target/release:$PATH"
```

## Usage

ANX source files use the `.nx` extension. Every program needs exactly one top-level `void main()`.

```bash
anx check solve.nx                       # lex + parse + type-check only
anx run solve.nx                         # instant interpreted execution
anx build solve.nx -o solve && ./solve   # compile to a genuine native binary
```

Exit codes: `0` success, `1` compile-time error, `2` runtime error — identical across the interpreted and compiled paths. Full walkthrough in [docs/P0/ANX-Usage-Flow-v1.md](docs/P0/ANX-Usage-Flow-v1.md).

## Testing

```bash
cargo test
```

150 unit tests (lexer, parser, sema, interpreter, codegen — including JIT-executed correctness checks, not just IR verification) + 8 CLI integration tests + the full 20-problem benchmark suite, each run through both the interpreter and a built-then-executed binary.

## Project layout

```
src/
├── lexer/      hand-written scanner
├── ast/        shared AST — walked by parser, sema, interpreter, and codegen
├── parser/     recursive-descent + precedence-climbing parser
├── sema/       shared semantic analysis (symbol table, type checking)
├── interp/     tree-walking interpreter
├── codegen/    LLVM IR codegen (inkwell) + the C runtime shim
└── cli.rs      anx check|run|build
tests/
├── benchmarks/ the 20 P0 benchmark programs + expected output
├── cli.rs      CLI-level smoke tests
└── integration.rs   dual-path (interpreter + compiled) benchmark suite
dogfood/        exploratory practice problems — not version-controlled
```

## Docs

Docs use a flat `docs/P0/` through `docs/P8/` scheme — one folder per major implementation slice, in build order. **This is not the same thing as the PRD's own P0/P1/P2 milestone labels** (which group several of these slices together) — the PRD and Syntax Draft are whole-project documents that happen to live under `P0/` since that's where they were authored, but the per-slice numbering below is purely sequencing. Interfaces, an AI Socratic tutor layer, an execution visualizer, and IDE tooling (the PRD's old "P2 milestone") have been dropped from the roadmap entirely, not deferred.

**P0 — [docs/P0/](docs/P0/) — the initial compiler, frozen**
- [Product Requirements Doc](docs/P0/ANX-PRD-v1.md), [Syntax Draft](docs/P0/ANX-Syntax-Draft-v1.md), [Implementation Plan](docs/P0/ANX-Implementation-Plan-v1.md), [Tech Stack](docs/P0/ANX-Tech-Stack-v1.md), [Usage Flow](docs/P0/ANX-Usage-Flow-v1.md), [Progress Tracker](docs/P0/ANX-Progress-v1.md) (Phases 0–8), [Dogfooding Notes](docs/P0/ANX-Dogfooding-Notes-v1.md) (the original 10-problem log)

**P1 — [docs/P1/](docs/P1/) — Operators (✅ done)**
- [Plan](docs/P1/ANX-P1-Operators-Plan-v1.md), [Tech Stack](docs/P1/ANX-P1-Tech-Stack-v1.md), [Usage Flow](docs/P1/ANX-P1-Usage-Flow-v1.md), [Progress](docs/P1/ANX-P1-Progress-v1.md), [Dogfooding Notes](docs/P1/ANX-P1-Dogfooding-Notes-v1.md)

**P2 — [docs/P2/](docs/P2/) — Strings (✅ done)**
- [Plan](docs/P2/ANX-P2-Strings-Plan-v1.md), [Tech Stack](docs/P2/ANX-P2-Tech-Stack-v1.md), [Usage Flow](docs/P2/ANX-P2-Usage-Flow-v1.md), [Progress](docs/P2/ANX-P2-Progress-v1.md), [Dogfooding Notes](docs/P2/ANX-P2-Dogfooding-Notes-v1.md) — length/index/concat/substring/equality

**P3 — [docs/P3/](docs/P3/) — Classes, non-generic (planned)**
- [Plan](docs/P3/ANX-P3-Classes-Plan-v1.md) — fields, constructors, methods, `this`; also holds the shared key decisions for P3–P6

**P4 — [docs/P4/](docs/P4/) — Generics (planned)**
- [Plan](docs/P4/ANX-P4-Generics-Plan-v1.md) — monomorphized on use

**P5 — [docs/P5/](docs/P5/) — Collections (planned)**
- [Plan](docs/P5/ANX-P5-Collections-Plan-v1.md) — `List`/`Stack`/`Queue`/`HashMap`, written in ANX itself as an auto-included prelude

**P6 — [docs/P6/](docs/P6/) — Tree/Graph (planned)**
- [Plan](docs/P6/ANX-P6-TreeGraph-Plan-v1.md) — built on classes + collections, not new primitives

**P7 — [docs/P7/](docs/P7/) — Diagnostics polish (planned)**
- [Plan](docs/P7/ANX-P7-Diagnostics-Plan-v1.md) — line:col everywhere, clearer messages

**P8 — [docs/P8/](docs/P8/) — Benchmark suite for P3–P6 (planned)**
- [Plan](docs/P8/ANX-P8-BenchmarkSuite-Plan-v1.md) — 10 new problems needing classes/generics/collections/Tree/Graph

## Roadmap

P1 (Operators) and P2 (Strings) are done. P3 (Classes) is next, then P4 (Generics) → P5 (Collections) → P6 (Tree/Graph) → P7 (Diagnostics) → P8 (benchmark suite for all of it) — see `docs/P3/` through `docs/P8/` for each slice's plan. Interfaces, an AI tutor layer, an execution visualizer, and IDE tooling are off the roadmap entirely.

---

Solo, nights-and-weekends project. No external deadline, no distribution plans yet — the only bar right now is being good enough that its own author reaches for it over Java during placement prep.
