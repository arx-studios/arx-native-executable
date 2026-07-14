# ANX — Dogfooding Notes (Phase 8)

**Companion to:** [ANX-PRD-v1.md](ANX-PRD-v1.md), [ANX-Progress-v1.md](ANX-Progress-v1.md)
**Purpose:** running log for PRD Goal 4 / the Implementation Plan's Phase 8 — Ayushman solving ≥10 *real* DSA practice problems in ANX (distinct from the fixed 20-program benchmark suite), tracking friction points as the primary input for what P1 actually needs beyond the PRD's current wishlist.

**This is a usage phase, not a coding phase.** The exit gate (10 problems solved without falling back to Java) is about real practice over time — nothing here should be mistaken for that being "done" by the tooling alone.

**⚠️ Important honesty note on entries 2–10 below:** the PRD's actual metric is *Ayushman* solving these, as a personal-preference signal ("does he reach for this over Java"). Entries 2–10 were written and solved by Claude, at Ayushman's explicit request, to get a language-*capability* data point quickly. That's real and useful signal (does ANX correctly handle 9 more varied real problems? — yes) but it is **not** the same signal the PRD metric is actually measuring, and shouldn't be quietly treated as satisfying it. The count below is tracked as two separate things for exactly this reason.

**Where problems live:** `dogfood/*.nx` (separate from `tests/benchmarks/`, which is the fixed, frozen 20-program regression suite — these are exploratory and expected to grow/change).

---

## Problem log

| # | Problem | Solved by | Result | Friction? |
|---|---|---|---|---|
| 1 | N-Queens (count solutions, n=8) | Claude | ✅ 92 | None |
| 2 | Sieve of Eratosthenes (primes ≤ 30) | Claude | ✅ 10 | None |
| 3 | Trapping rain water | Claude | ✅ 6 | None |
| 4 | Rotate array right by k | Claude | ✅ `[5,6,7,1,2,3,4]` | None |
| 5 | Move zeroes to end (stable) | Claude | ✅ `[1,3,12,0,0]` | None |
| 6 | Count inversions (modified merge sort) | Claude | ✅ 3 | None |
| 7 | Josephus problem (recursive) | Claude | ✅ 3 | None |
| 8 | Matrix transpose (2D flattened to 1D) | Claude | ✅ `[1,3,5,2,4,6]` | None |
| 9 | Single number (one non-duplicate) | Claude | ✅ 4 | **Yes — see below** |
| 10 | Kth largest via quickselect | Claude | ✅ 5 | None |

**Language-capability count: 10/10** (all pass on both interpreter and compiled path, no crashes, no mismatches).
**PRD-metric count (Ayushman's own practice): 1/10** — only entry 1 (N-Queens) was actually done by Ayushman as personal practice before entries 2–10 were fast-tracked by Claude on request. **The real exit gate is still open** — this log will need genuine entries from Ayushman's own placement-prep practice to actually close it, whether that's replacing these or adding fresh ones on top.

---

## Friction found

### Entry 9 — Single Number: no bitwise ops, no collections forces an O(n²) fallback

The idiomatic solutions to "every element appears twice except one, find it" are XOR-fold (O(n), O(1) space, needs a bitwise XOR operator) or a hash-count (O(n), needs a map). **ANX's P0 grammar has neither** — no bitwise operators at all (`& | ^ << >>` don't exist, confirmed against the lexer's operator set), and no `HashMap` (P1, not yet built). The problem had to be solved with a brute-force O(n²) nested-loop count instead:

```
int singleNumber(int[] arr) {
    for (int i = 0; i < arr.length; i = i + 1) {
        int count = 0;
        for (int j = 0; j < arr.length; j = j + 1) {
            if (arr[j] == arr[i]) count = count + 1;
        }
        if (count == 1) return arr[i];
    }
    return -1;
}
```

**Why this matters for P1/P2 prioritization:** the current P1 wishlist (PRD) covers collections/generics/classes but doesn't mention bitwise operators at all. A meaningful class of classic DSA problems (bit manipulation: single number, subsets via bitmask, counting set bits, XOR tricks) is currently unreachable in better than brute-force time — not because collections are missing, but because there's no bitwise operator grammar. Worth a line item in whatever comes after this P1 pass, even though it's out of scope for the current P1 plan.

### Everything else: no friction

The other 8 problems (sieve, trapping rain water, array rotation, move-zeroes, count-inversions, Josephus, matrix transpose, quickselect) all ran correctly on the first try on both paths, with no missing-feature workarounds. Notably exercised, without issue: a function *returning* a freshly-allocated array (matrix transpose — not used by anything in the fixed benchmark suite, which only ever mutates arrays passed in or returns scalars), two array parameters threaded through mutual recursion (count-inversions), and a decrementing for-loop with a `>=` condition (trapping rain water's right-to-left pass).

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

**Takeaway:** no P1 signal from this one — it's a clean data point, not a gap.

---

## How to log a new entry

1. Write the problem under `dogfood/`.
2. Solve it in ANX; run both `anx run` and `anx build` + execute.
3. Add a row to the table above, and record who actually solved it (this matters — see the honesty note up top).
4. If anything was awkward, missing, or confusing — even if you worked around it — add an entry under "Friction found" with what happened and what would need to change to address it. Friction is the point of this phase; a silent workaround here is a missed signal for prioritizing what actually blocks real usage.
