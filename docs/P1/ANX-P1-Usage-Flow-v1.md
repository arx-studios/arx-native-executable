# ANX — P1 Usage Flow (v1)

**Companion to:** [ANX-Usage-Flow-v1.md](../P0/ANX-Usage-Flow-v1.md) (P0), [ANX-P1-Operators-Plan-v1.md](ANX-P1-Operators-Plan-v1.md)
**Scope:** P1 = Operators only. Once P2 (strings), P3 (classes), P4 (generics), and P5 (collections) actually ship, each gets its own usage-flow notes in its own folder rather than being previewed here ahead of time.
**Purpose:** what using the new operator syntax actually looks like. The CLI itself (`anx check|run|build`, exit codes, `.nx` extension) is unchanged from P0's [Usage Flow doc](../P0/ANX-Usage-Flow-v1.md) — this doc is purely about new *language* surface, not new tooling.

---

## Operators — ✅ implemented

```
// Ternary
int max = (a > b) ? a : b;

// Compound assignment
count += 1;
total *= 2;
mask &= 15;        // decimal only — no hex literals in the grammar
flags |= 1;
bits <<= 2;

// Bitwise / shift
int x = a & b;
int y = a | b;
int z = a ^ b;
int w = ~a;
int lo = n << 1;
int hi = n >> 1;       // arithmetic (sign-extending)
int hi2 = n >>> 1;      // logical (zero-filling) — differs from >> only when n is negative
n >>>= 1;

// Increment / decrement
int p = 5;
print(++p);   // 6 — prefix returns the new value
print(p++);   // 6 — postfix returns the value *before* the change
print(p);     // 7
arr[i]++;     // works on array-index targets too, evaluates i once
```

All of the above work identically on `anx run` and a compiled binary — verified directly, including that a compound-assignment or increment/decrement target's array index (e.g. `arr[f()] += 1`, `arr[f()]++`) is only evaluated once.

**Not implemented, on purpose:** `instanceof`. It needs a class hierarchy with subtyping to check against, but the roadmap ([P3 Classes Plan](../P3/ANX-P3-Classes-Plan-v1.md)) has already decided against interfaces/inheritance/virtual dispatch — without subtyping, there's no real question left for `instanceof` to answer in ANX.
