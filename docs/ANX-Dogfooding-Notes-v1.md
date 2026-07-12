# ANX — Dogfooding Notes (Phase 8)

**Companion to:** [ANX-PRD-v1.md](ANX-PRD-v1.md), [ANX-Progress-v1.md](ANX-Progress-v1.md)
**Purpose:** running log for PRD Goal 4 / the Implementation Plan's Phase 8 — Ayushman solving ≥10 *real* DSA practice problems in ANX (distinct from the fixed 20-program benchmark suite), tracking friction points as the primary input for what P1 actually needs beyond the PRD's current wishlist.

**This is a usage phase, not a coding phase.** The exit gate (10 problems solved without falling back to Java) is about real practice over time — nothing here should be mistaken for that being "done" by the tooling alone.

**Where problems live:** `dogfood/*.nx` (separate from `tests/benchmarks/`, which is the fixed, frozen 20-program regression suite — these are exploratory and expected to grow/change).

---

## Problem log

| # | Problem | Result | Friction? |
|---|---|---|---|
| 1 | N-Queens (count solutions, n=8) | ✅ 92 (correct), both interpreter and compiled path | None |

**10/10 needed. 1 logged so far** (see below for how this one was seeded) — 9 more real problems from Ayushman's own placement-prep practice needed to actually close this phase's exit gate.

---

## Entry 1 — N-Queens (`dogfood/nqueens.nx`)

Chosen to seed this log with a first real workflow pass and pick something structurally *unlike* anything in the 20-benchmark suite: full backtracking (place → recurse → let the next loop iteration's overwrite implicitly "undo" the placement), rather than the suite's iterative/divide-and-conquer/DP patterns.

```
bool isSafe(int[] board, int row, int col) {
    for (int i = 0; i < row; i = i + 1) {
        int c = board[i];
        if (c == col) return false;
        int diff = row - i;
        if (c - col == diff) return false;
        if (col - c == diff) return false;
    }
    return true;
}

int solve(int[] board, int row, int n) {
    if (row == n) return 1;
    int count = 0;
    for (int col = 0; col < n; col = col + 1) {
        if (isSafe(board, row, col)) {
            board[row] = col;
            count = count + solve(board, row + 1, n);
        }
    }
    return count;
}

void main() {
    int n = 8;
    int[] board = new int[n];
    print(solve(board, 0, n));
}
```

**Result:** `92` (the known correct count for 8-queens) — matched on both `anx run` and a built-then-executed binary, first try.

**Friction:** none. Specifically exercised, without issue:
- A `bool`-returning helper function called from within a loop condition.
- An `int[]` parameter shared and mutated across a chain of recursive calls (`solve` → `isSafe`, `solve` → `solve`), relying on ANX's array reference semantics — this is the same by-value-struct-wrapping-a-shared-heap-buffer design from Phase 5, now exercised by a genuinely different call pattern (backtracking) than anything in the fixed suite.
- Early `return false` from deep inside a nested loop-within-a-function-within-a-loop.

**Takeaway:** no P1 signal from this one — it's a clean data point, not a gap. Real signal will come from Ayushman's own harder/weirder problems (this one was deliberately picked to be *plausible* for the current language, not adversarial).

---

## How to log a new entry

1. Write the problem under `dogfood/`.
2. Solve it in ANX; run both `anx run` and `anx build` + execute.
3. Add a row to the table above.
4. If anything was awkward, missing, or confusing — even if you worked around it — add an entry below with what happened and what P1 (or even a P0 fix) would need to address it. Friction is the point of this phase; a silent workaround here is a missed signal for prioritizing what actually blocks real usage.
