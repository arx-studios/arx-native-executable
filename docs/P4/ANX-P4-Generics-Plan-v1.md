# ANX — P4 Generics Implementation Plan (v1)

**Companion to:** [ANX-P3-Classes-Plan-v1.md](../P3/ANX-P3-Classes-Plan-v1.md) (P3 — shared key decisions §1, grammar §2), [ANX-P5-Collections-Plan-v1.md](../P5/ANX-P5-Collections-Plan-v1.md)

**Scope:** Generic classes, monomorphized on use. Depends on P3 (classes) shipping first — generics only make sense once there's a class system to parameterize.

**Numbering note:** see [P3's numbering note](../P3/ANX-P3-Classes-Plan-v1.md) — flat `P0`–`P8` slices, not the same as the PRD's own milestone labels.

---

## Key decisions

Already made in [P3 §1](../P3/ANX-P3-Classes-Plan-v1.md#1-key-engineering-decisions): monomorphization is the implementation strategy (confirmed from the P0 plan's non-blocking note on generics), not type erasure. This doc just executes it.

## Phase plan

- Grammar/AST: generic type parameters on `ClassDecl` (`class Stack<T> { ... }`), generic instantiation in `type` (`Stack<int>`) — the grammar productions already exist per P3 §2, this phase is what makes the parser actually *do* something with them instead of ignoring them.
- Sema: when a generic class is instantiated with concrete type arguments for the first time in a program, monomorphize — substitute the type parameter(s) throughout a copy of the class's AST and type-check the result as an ordinary (now-concrete) class. Cache by `(class_name, type_args)` so repeated instantiations with the same arguments reuse one compiled version.
- Codegen: each distinct monomorphized instantiation gets its own LLVM struct + functions (e.g. `Stack<int>` and `Stack<string>` are entirely separate generated types/functions, no shared generic representation at the IR level — this is the whole point of monomorphization).
- **Exit gate:** a hand-written generic `Box<T>` (single field of type `T`, a getter and setter) instantiated as both `Box<int>` and `Box<bool>` in the same program compiles and runs correctly on both paths.

## Risks

| Risk | Mitigation |
|---|---|
| Monomorphization blowing up compile time/binary size if a program instantiates many distinct generic type combinations | Not a real concern at DSA-practice scale (a handful of instantiations per program, small programs) — revisit only if dogfooding ever surfaces an actual slow build. |
