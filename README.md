# ANX — Product Requirements Document (v1)

*ANX: Arx Native eXecutable*

**Owner:** Ayushman Das, ARX Studios
**Status:** Draft
**Last updated:** July 8, 2026

## Problem Statement

CS students learning data structures and algorithms typically practice in general-purpose languages (Python, Java, C++) that weren't designed for the task. Python hides memory and complexity behavior behind abstraction; Java and C++ require heavy boilerplate for structures (stacks, queues, trees, graphs) that DSA problems use constantly. There's no language purpose-built for algorithm practice that's also a real, compiled language — not a toy interpreter — with the data structures DSA problems actually need built in as first-class citizens. Every serious CS student globally hits this friction; the cost is wasted setup time and shallow understanding of what their code actually does at runtime.

## Goals

1. Ship a working ANX v1 compiler (interpreter + LLVM-backed native compilation) that correctly runs a 20-problem benchmark suite of classic DSA patterns end to end.
2. Support a minimal v1 language surface first: variables, control flow, functions/recursion, and arrays — proving the full pipeline end to end before adding collections or classes.
3. Validate the language is genuinely compiled, not just interpreted — produce real native binaries via LLVM for the benchmark suite.
4. Personally dogfood ANX for real DSA practice (Ayushman uses it for at least 10 problems) to validate usability before considering any external audience.
5. Establish a clean, extensible compiler architecture (shared AST/semantic analysis feeding both the interpreter and the LLVM backend) so P1 features don't require a rewrite.

## Non-Goals (v1)

1. **AI tutoring / Socratic questioning layer** — explicitly deferred; the core language has to work and be worth using before layering AI on top.
2. **Execution visualizer** — explicitly deferred for the same reason; a real differentiator, but not the v1 wedge.
3. **IDE tooling** (syntax highlighting, LSP, autocomplete) — compiler correctness comes first; tooling is worthless on top of a language that doesn't work yet.
4. **Multi-file projects / module system** — DSA problems are single-file by nature; no need to solve project structure yet.
5. **Distribution, marketing, or external users** — v1's success criterion is "does this actually work and is it worth using," not adoption.

## User Stories

- As a CS student practicing DSA, I want real recursion support so I can implement DFS, backtracking, and divide-and-conquer naturally.
- As a CS student, I want built-in Stack, Queue, and HashMap types so I'm not hand-rolling data structures to solve a problem about algorithms, not plumbing.
- As a CS student, I want my program to compile to a real native binary so I know I'm working with a genuine language, not a toy.
- As a CS student iterating on a solution, I want instant interpreted execution so I'm not waiting on a compile step every time I test a change.
- As the language's author, I want a benchmark suite of real DSA problems implemented in ANX so I can verify the language is actually complete enough to be useful, not just technically working.

## Requirements

### Must-Have (P0) — the basics
- Lexer + parser for Java-like ANX syntax
- AST + shared semantic analysis pass
- Tree-walking interpreter for instant execution
- LLVM IR codegen for: variables, arithmetic/logic, control flow (if/else, while, for), functions, recursion, arrays
- Compile pipeline: ANX source → LLVM IR → native binary via LLVM's backend
- 15–20 problem benchmark suite passing on both interpreter and compiled paths, using only primitives/arrays/recursion (binary search, two-pointer, sorting, basic DP) — no built-in collections required

### Next Phase (P1) — data structures & OOP
- Built-in List, Stack, Queue, HashMap
- Generics for collections (`List<T>`, `Stack<T>`)
- Classes: fields, constructors, methods
- Tree and Graph as standard-library types
- Clearer compiler error messages with line numbers

### Future Considerations (P2)
- Interfaces and virtual dispatch
- AI Socratic tutor layer
- Step-by-step execution visualizer
- C++ transpile output as a "see the real code" teaching artifact
- IDE integration / LSP
- Package/module system

## Success Metrics

**Leading:** % of the 20-problem benchmark suite that compiles and runs correctly — target 100% on the P0 feature set before calling v1 done.
**Leading:** Ayushman solves at least 10 real DSA problems in ANX during personal placement prep, without falling back to Java.
**Lagging:** Whether ANX keeps getting used for practice a month after v1 ships, unprompted — the only adoption signal that matters at N=1 before any external users exist.

## Open Questions

- **(Engineering, blocking)** Memory model: garbage-collected like Java, manual like C++, or a simplified "leak on exit" model appropriate for short-lived DSA scripts? This affects both syntax and the LLVM codegen design.
- **(Engineering, blocking)** Do the interpreter and LLVM backend share one semantic analysis pass, or diverge? Sharing is less work long-term but couples the two paths early.
- **(Engineering, non-blocking)** Generics implementation strategy for LLVM — monomorphization (like Rust/C++ templates) vs. type erasure (like Java)? Can defer to the P1 phase.
- **(Design, next deliverable)** Full grammar and syntax — drafted as a companion doc alongside this PRD.

## Timeline Considerations

No external deadline. The real constraint is solo-dev bandwidth during placement season, capstone wrap-up, and an active internship — this is nights/weekends work. Suggested phase order: frontend (lexer/parser/AST) → interpreter → LLVM codegen for primitives/control-flow/functions/arrays → basics benchmark suite passing → only then collections (P1) → only then classes/generics (P1).
