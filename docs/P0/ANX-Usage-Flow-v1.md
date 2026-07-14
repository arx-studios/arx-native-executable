# ANX — Usage Flow (v1)

**Companion to:** [ANX-PRD-v1.md](ANX-PRD-v1.md), [ANX-Syntax-Draft-v1.md](ANX-Syntax-Draft-v1.md), [ANX-Implementation-Plan-v1.md](ANX-Implementation-Plan-v1.md), [ANX-Tech-Stack-v1.md](ANX-Tech-Stack-v1.md), [ANX-Progress-v1.md](ANX-Progress-v1.md)
**Scope:** what it's like to actually use ANX once P0 is done. No distribution/packaging (PRD non-goal #5) — this describes running `anx` as a locally built tool, not something installed from a package registry.

---

## File extension

`.nx` — e.g. `binary_search.nx`.

## Entry point

Every `.nx` file must define exactly one top-level `void main()`. That's what runs first, whether the file is interpreted or compiled — a Java/C-style convention, chosen in [ANX-Implementation-Plan-v1.md §1](ANX-Implementation-Plan-v1.md#1-key-engineering-decisions) since the syntax draft didn't specify one:

```
// solve.nx
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

## Two ways to execute — this is the core of the v1 flow

The PRD's whole premise is "instant interpreted iteration, real compiled binary when it counts" (PRD Goals 1 & 3, and the corresponding user stories). That maps to two CLI commands over the same source file:

| Command | Path | When to use |
|---|---|---|
| `anx run solve.nx` | Lex → parse → sema → tree-walking interpreter, all in one process | Every edit-run cycle while writing/debugging a solution — no compile step, output in milliseconds. |
| `anx build solve.nx -o solve` then `./solve` | Lex → parse → sema → LLVM IR → object file → linked native executable | Once the solution works, to get (and prove) a real standalone binary. |
| `anx check solve.nx` | Lex → parse → sema only, no execution | Fast syntax/type feedback without running anything — e.g. wired into an editor's save hook later, though no editor integration exists in v1. |

Both `run` and `build` share the exact same frontend and sema pass (per the Implementation Plan's shared-AST decision), so a program that type-checks once behaves identically on both paths — that consistency is itself part of what the benchmark suite (Phase 7) verifies.

## Walkthrough

```
$ anx run solve.nx
7

$ anx build solve.nx -o solve
$ ./solve
7

$ file solve
solve: Mach-O 64-bit executable arm64
```

The `file solve` step isn't ceremony — it's the literal check for PRD Goal 3 ("validate the language is genuinely compiled ... not just interpreted").

Typical DSA-practice loop: `anx run` repeatedly while iterating on a solution, then a single `anx build` + `file` at the end once it's correct, mirroring the dogfooding step in the Implementation Plan (Phase 8).

## Errors and exit codes

- **Compile-time errors** (lex/parse/sema failures — undeclared identifier, type mismatch, missing `main`, etc.): reported to stderr with line numbers, process exits `1`, nothing runs. Same on `run`, `build`, and `check`.
- **Runtime errors** (array out of bounds, division by zero): reported to stderr, process exits `2`. Only reachable via `run` or a built binary — `check` never executes code so can't hit these.
- **Success**: exit `0`.

```
$ anx run broken.nx
error: broken.nx:4: undeclared identifier 'target'
$ echo $?
1
```

## Installing / building `anx` itself

No package manager or installer in v1 — build from source and put the binary on `PATH`:

```
git clone <this repo>
cd anx
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

## Explicitly not part of v1

- **No REPL.** File-based only — matches "DSA problems are single-file by nature" (PRD non-goal #4) and keeps the CLI surface to the three commands above.
- **No program input** (stdin/argv) — v1 benchmark problems hardcode their inputs in `main()` and check `print()` output, per how the benchmark suite (Implementation Plan Phase 7) is structured. Worth revisiting once dogfooding (Phase 8) hits a problem that genuinely wants external input.
- **No watch mode / editor integration** — IDE tooling is an explicit PRD non-goal for v1.
