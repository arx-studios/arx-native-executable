# ANX — P1 Dogfooding Notes (v1)

**Companion to:** [ANX-Dogfooding-Notes-v1.md](../P0/ANX-Dogfooding-Notes-v1.md) (P0 — the original 10-entry log), [ANX-P1-Progress-v1.md](ANX-P1-Progress-v1.md)
**Purpose:** a fresh dogfooding log for problems that specifically exercise *new P1 features* as they land (real strings, bitwise/shift operators, ternary, classes, generics, collections) — kept separate from the P0 log rather than appended to it, since the P0 log is frozen as the record of what P0-only ANX could do.

**Status: Operators entries logged.** See [ANX-P1-Progress-v1.md](ANX-P1-Progress-v1.md) for what's actually shipped. Same honesty note as the P0 log applies: entries below were solved by Claude verifying the feature works, not by Ayushman's own practice — real signal on language capability, not on the "does he reach for it" preference signal the PRD actually cares about.

**Still pending, once strings ship:** string problems entirely unreachable in P0 — valid anagram, longest common prefix, "build the reversed string" — per the [P2 Strings Plan](../P2/ANX-P2-Strings-Plan-v1.md)'s own Step 5. Strings has been resequenced out of P1 into P2, so this is no longer next up — classes/generics/collections come first now.

---

## Problem log

| # | Problem | Feature(s) exercised | Solved by | Result | Friction? |
|---|---|---|---|---|---|
| 1 | Single Number, re-solved | `^=` (compound XOR-assign) | Claude | ✅ 4, both paths | None — closes the P0 log's entry 9 friction point |
| 2 | Power-of-two check + combined ops | ternary, `&`, `<<=`, `+=`, `&&` | Claude | ✅ `23`/`yes`/`no`, both paths | None |

---

## Entry 1 — Single Number, re-solved with real XOR

The [P0 log's entry 9](../P0/ANX-Dogfooding-Notes-v1.md) found that "Single Number" (every element appears twice except one) had no idiomatic solution in P0 — no bitwise XOR, no hash map — forcing an O(n²) brute-force fallback. With bitwise operators now shipped:

```
int singleNumber(int[] arr) {
    int result = 0;
    for (int i = 0; i < arr.length; i = i + 1) {
        result ^= arr[i];
    }
    return result;
}

void main() {
    int[] arr = [4, 1, 2, 1, 2];
    print(singleNumber(arr));
}
```

**Result:** `4` (correct) on both `anx run` and a built-then-executed binary. **Friction: none** — this is exactly the idiomatic O(n) solution now, closing the gap.

## Entry 2 — Power-of-two check + combined operators

Deliberately exercises several new operators together in one program, not in isolation:

```
bool isPowerOfTwo(int n) {
    return n > 0 && (n & (n - 1)) == 0;
}

void main() {
    int x = 5;
    x <<= 2;
    x += 3;
    print(x);
    print(isPowerOfTwo(16) ? "yes" : "no");
    print(isPowerOfTwo(18) ? "yes" : "no");
}
```

**Result:** `23`, `yes`, `no` — correct on both paths. **Friction: none.**

---

## How to log a new entry

Same format as the [P0 log](../P0/ANX-Dogfooding-Notes-v1.md): write the problem, run both `anx run` and `anx build` + execute, record who actually solved it (the P0 log's honesty note about Claude-solved vs. Ayushman-solved entries applies here too), and log any friction — even a workaround that "technically worked" — since that's the actual point of dogfooding.
