# ANX

*ANX: Arx Native eXecutable*

A small, real compiled language purpose-built for practicing data structures and algorithms — Java-like syntax, instant tree-walking interpretation for fast iteration, and genuine LLVM-backed native compilation so it's never just a toy.

## Why

CS students practicing DSA usually reach for Python, Java, or C++ — none of which were designed for the job. Python hides memory/runtime behavior behind abstraction; Java and C++ demand heavy boilerplate for the exact structures (stacks, queues, trees, graphs) DSA problems use constantly. ANX is an attempt at a language purpose-built for this: real compiled semantics, DSA-shaped built-ins, no setup ceremony. Full rationale in the [PRD](docs/ANX-PRD-v1.md).

## Status

**P0 is complete** — lexer through native codegen, all 20 benchmark problems passing on both the interpreter and the compiled path. Currently in Phase 8: dogfooding (real DSA practice, tracking friction for P1). [docs/ANX-Progress-v1.md](docs/ANX-Progress-v1.md) is the authoritative, continuously-updated source of truth for what's actually built — treat anything below as a snapshot, not a promise.

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

Exit codes: `0` success, `1` compile-time error, `2` runtime error — identical across the interpreted and compiled paths. Full walkthrough in [docs/ANX-Usage-Flow-v1.md](docs/ANX-Usage-Flow-v1.md).

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

- [Product Requirements Doc](docs/ANX-PRD-v1.md) — problem statement, goals, P0/P1/P2 scope
- [Syntax Draft](docs/ANX-Syntax-Draft-v1.md) — language grammar and worked examples
- [Implementation Plan](docs/ANX-Implementation-Plan-v1.md) — phase-by-phase build plan and key engineering decisions
- [Tech Stack](docs/ANX-Tech-Stack-v1.md) — exactly what's used at each step, and why
- [Usage Flow](docs/ANX-Usage-Flow-v1.md) — the full CLI / user-facing walkthrough
- [Progress Tracker](docs/ANX-Progress-v1.md) — what's actually built, phase by phase
- [Dogfooding Notes](docs/ANX-Dogfooding-Notes-v1.md) — real-practice log and friction points

## Roadmap

P1 (built-in `List`/`Stack`/`Queue`/`HashMap`, generics, classes) and P2 (interfaces, an AI Socratic tutor layer, an execution visualizer, IDE tooling) are scoped in the [PRD](docs/ANX-PRD-v1.md) but not started — P0 has to actually be worth using first.

---

Solo, nights-and-weekends project. No external deadline, no distribution plans yet — the only bar right now is being good enough that its own author reaches for it over Java during placement prep.
