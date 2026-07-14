# ANX — P6 Tree/Graph Implementation Plan (v1)

**Companion to:** [ANX-P3-Classes-Plan-v1.md](../P3/ANX-P3-Classes-Plan-v1.md) (P3), [ANX-P5-Collections-Plan-v1.md](../P5/ANX-P5-Collections-Plan-v1.md) (P5)

**Scope:** `Tree` and `Graph` as standard-library types. Depends on P3 (classes) and P5 (collections) both shipping first.

**Numbering note:** see [P3's numbering note](../P3/ANX-P3-Classes-Plan-v1.md).

---

## Phase plan

Built on top of classes/collections, not new primitives, per the original P0 plan's own guidance:
- `Tree<T>` as a `Node<T> { T value; Node<T> left; Node<T> right; }`-style class.
- `Graph` as an adjacency list (`List<List<int>>` or similar) rather than a hand-rolled matrix.
- **Exit gate:** a simple BST insert + in-order traversal, and a BFS over a small graph, both run correctly on both paths.
