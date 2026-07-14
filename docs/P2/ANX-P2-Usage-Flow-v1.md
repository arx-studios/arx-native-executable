# ANX — P2 Usage Flow (v1)

**Companion to:** [ANX-Usage-Flow-v1.md](../P0/ANX-Usage-Flow-v1.md) (P0), [ANX-P2-Strings-Plan-v1.md](ANX-P2-Strings-Plan-v1.md)
**Scope:** P2 = Strings only. Once P3 (classes), P4 (generics), and P5 (collections) actually ship, each gets its own usage-flow notes in its own folder rather than being previewed here ahead of time.
**Purpose:** what using the new string syntax actually looks like. The CLI itself (`anx check|run|build`, exit codes, `.nx` extension) is unchanged from P0's [Usage Flow doc](../P0/ANX-Usage-Flow-v1.md) — this doc is purely about new *language* surface, not new tooling.

---

## Strings — ✅ implemented

```
string s = "hello";

// length — free function, or field access (both do the same thing,
// counting bytes not Unicode scalars — see the plan's §5)
int n = length(s);
int n2 = s.length;

// charAt — bounds-checked, returns a length-1 string (no `char` type)
string c = charAt(s, 0);

// substring — [start, end), like Java; bounds-checked
string sub = substring(s, 1, 4);

// concatenation
string greeting = "hello" + " " + "world";

// content comparison, not reference comparison
bool same = s == "hello";
bool different = s != "world";
```

A typical DSA pattern this unblocks — palindrome check by walking in from both ends:

```
bool isPalindrome(string s) {
    int i = 0;
    int j = length(s) - 1;
    while (i < j) {
        if (charAt(s, i) != charAt(s, j)) return false;
        i = i + 1;
        j = j - 1;
    }
    return true;
}
```

All of the above work identically on `anx run` and a compiled binary — verified directly, including that an out-of-bounds `charAt`/`substring` call produces the exact same `runtime error: string index N out of bounds for length M` message and exit code `2` on both paths.

**Not implemented, on purpose** (all carried forward explicitly from the plan, not silently dropped):
- **`s.length()`/`s.charAt(i)` method-call syntax** — `length`/`charAt`/`substring` are free functions for now, since method-call parsing doesn't exist until P3 (classes). Revisit once P3 ships, per the plan's own stated follow-up.
- **A distinct `char` type** — `charAt` returns a length-1 `string` instead. Adding `char` would mean a new lexer token, a new `Type` variant, and a `char` vs. `int` arithmetic decision; not worth it when every DSA use (`charAt(s,i) == "a"`) already reads naturally as string comparison.
- **String mutation / a `StringBuilder` equivalent** — strings are immutable, Java-`String`-style; every concat/substring allocates a new buffer. Revisit only if a real dogfooding problem hits a performance wall at DSA-input scale (unlikely).
- **`split()`, `compareTo()`, regex, string formatting** — none of these came up as blocking; add only if a specific future dogfooding problem actually needs one.
