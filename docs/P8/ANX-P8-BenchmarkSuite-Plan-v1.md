# ANX — P8 Benchmark Suite Plan (v1)

**Companion to:** [ANX-P3-Classes-Plan-v1.md](../P3/ANX-P3-Classes-Plan-v1.md), [ANX-P5-Collections-Plan-v1.md](../P5/ANX-P5-Collections-Plan-v1.md), [ANX-P6-TreeGraph-Plan-v1.md](../P6/ANX-P6-TreeGraph-Plan-v1.md)

**Scope:** new benchmark problems that specifically need what P3–P6 add (classes, generics, collections, Tree/Graph) — the existing 20 in `tests/benchmarks/` stay frozen as the P0 regression suite; these are new, separate fixtures. Depends on P3, P5, and P6 all shipping first.

**Numbering note:** see [P3's numbering note](../P3/ANX-P3-Classes-Plan-v1.md).

---

## Phase plan

1. Valid parentheses matching (`Stack<char>` — note: needs a `char` type or reuses `string` of length 1; decide during P2/P4)
2. Implement a queue using two stacks
3. BFS shortest path on a small graph (`Graph` + `Queue<int>`)
4. DFS connected-components count (`Graph`, recursion)
5. Binary search tree insert + in-order traversal (`Tree`)
6. Reverse a singly linked list (plain `class Node`, no collections needed)
7. Detect a cycle in a linked list (two-pointer + `class Node`)
8. LRU cache (fixed capacity) using `HashMap` + `List`
9. Group anagrams / count character frequency using `HashMap<string, int>`
10. Level-order tree traversal using `Tree` + `Queue`

Each gets the same `.nx` + `.expected` dual-path treatment as the P0 suite, reusing `tests/integration.rs`'s structure.

**Exit gate:** all 10 pass on both the interpreter and compiled paths.
