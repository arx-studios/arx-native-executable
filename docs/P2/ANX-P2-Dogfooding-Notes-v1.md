# ANX — P2 Dogfooding Notes (v1)

**Companion to:** [ANX-P1-Dogfooding-Notes-v1.md](../P1/ANX-P1-Dogfooding-Notes-v1.md) (P1), [ANX-P2-Strings-Plan-v1.md](ANX-P2-Strings-Plan-v1.md), [ANX-P2-Progress-v1.md](ANX-P2-Progress-v1.md)
**Purpose:** Step 5 of the Strings plan — string-shaped DSA problems that were entirely unreachable in P0/P1 (no `length`/`charAt`/`substring`/`+`/`==` existed), now that P2 ships them. Same honesty note as the P0 and P1 logs: entries below were solved by Claude verifying the feature works end-to-end, not by Ayushman's own practice — real signal on language capability, not on the "does he reach for it" preference signal the PRD actually cares about.

---

## Problem log

| # | Problem | Feature(s) exercised | Solved by | Result | Friction? |
|---|---|---|---|---|---|
| 1 | Valid Anagram | `length`, `charAt`, `==`, `!=` | Claude | ✅ `true`/`false`, both paths | None |
| 2 | Longest Common Prefix | `length`, `charAt`, `substring`, `\|\|`, `!=` | Claude | ✅ `fl`/`""`, both paths | None |
| 3 | Build the Reversed String | `length`, `charAt`, `+` (concat) | Claude | ✅ `olleh`/`""`, both paths | None |

---

## Entry 1 — Valid Anagram

No `char`, no int-array-of-counts shortcut needed (P0 arrays would work too, but this stays purely string-based) — counts each letter of the 26-letter alphabet by scanning both strings with `charAt` and `==`:

```
int countChar(string s, string c) {
    int count = 0;
    int i = 0;
    while (i < length(s)) {
        if (charAt(s, i) == c) count = count + 1;
        i = i + 1;
    }
    return count;
}

bool isAnagram(string a, string b) {
    if (length(a) != length(b)) return false;
    string alphabet = "abcdefghijklmnopqrstuvwxyz";
    int k = 0;
    while (k < length(alphabet)) {
        string c = charAt(alphabet, k);
        if (countChar(a, c) != countChar(b, c)) return false;
        k = k + 1;
    }
    return true;
}

void main() {
    print(isAnagram("anagram", "nagaram"));
    print(isAnagram("rat", "car"));
}
```

**Result:** `true`, `false` — correct on both `anx run` and a built-then-executed binary. **Friction: none.**

## Entry 2 — Longest Common Prefix

Exercises `substring` for the actual prefix extraction, not just `charAt`:

```
string longestCommonPrefix(string a, string b, string c) {
    int minLen = length(a);
    if (length(b) < minLen) minLen = length(b);
    if (length(c) < minLen) minLen = length(c);
    int i = 0;
    while (i < minLen) {
        string ch = charAt(a, i);
        if (charAt(b, i) != ch || charAt(c, i) != ch) return substring(a, 0, i);
        i = i + 1;
    }
    return substring(a, 0, minLen);
}

void main() {
    print(longestCommonPrefix("flower", "flow", "flight"));
    print(longestCommonPrefix("dog", "racecar", "car"));
}
```

**Result:** `fl`, then an empty line (empty-string prefix — no common prefix at all) — correct on both paths, including the `substring(a, 0, 0)` empty-slice edge case. **Friction: none.**

## Entry 3 — Build the Reversed String

The immutable-strings version of in-place reversal — per the plan's own naming ("well — immutable, so 'build the reversed string'"), this accumulates via `+` instead of mutating in place:

```
string reverseString(string s) {
    string result = "";
    int i = length(s) - 1;
    while (i >= 0) {
        result = result + charAt(s, i);
        i = i - 1;
    }
    return result;
}

void main() {
    print(reverseString("hello"));
    print(reverseString(""));
}
```

**Result:** `olleh`, then an empty line for the empty-string case (loop body never runs since `i` starts at `-1`) — correct on both paths. **Friction: none** — immutability didn't cost anything here since DSA-scale strings are short; a hypothetical `StringBuilder` would only matter at a much larger scale than any practice problem reaches.

---

## How to log a new entry

Same format as the [P1 log](../P1/ANX-P1-Dogfooding-Notes-v1.md): write the problem, run both `anx run` and `anx build` + execute, record who actually solved it, and log any friction — even a workaround that "technically worked" — since that's the actual point of dogfooding.
