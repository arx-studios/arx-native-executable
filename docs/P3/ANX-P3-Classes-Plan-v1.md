# ANX — P3 Classes Implementation Plan (v1)

**Companion to:** [ANX-PRD-v1.md](../P0/ANX-PRD-v1.md), [ANX-Implementation-Plan-v1.md](../P0/ANX-Implementation-Plan-v1.md) (P0), [ANX-Syntax-Draft-v1.md](../P0/ANX-Syntax-Draft-v1.md) (P0), [ANX-P1-Operators-Plan-v1.md](../P1/ANX-P1-Operators-Plan-v1.md) (P1, done), [ANX-P2-Strings-Plan-v1.md](../P2/ANX-P2-Strings-Plan-v1.md) (P2), [ANX-P4-Generics-Plan-v1.md](../P4/ANX-P4-Generics-Plan-v1.md), [ANX-P5-Collections-Plan-v1.md](../P5/ANX-P5-Collections-Plan-v1.md)

**Scope:** Non-generic classes — fields, constructors, methods, `this`, `new ClassName(args)`. First of the class-system slices; generics (P4) and collections (P5) build directly on this.

**Numbering note:** ANX's phase docs now use a flat `P0, P1, P2, ...` scheme, one number per major feature slice, matching the `docs/P0`–`docs/P8` folder structure — not the same thing as the PRD's own P0/P1/P2 *milestone* labels (which group several of these slices together). Interfaces/AI-tutor/visualizer/IDE tooling (the PRD's old P2 milestone) have been dropped from the roadmap entirely, not deferred.

---

## 1. Key engineering decisions

These decisions are shared by every class-system slice (P3–P6), recorded here since Classes is the foundational one — P4/P5/P6 reference back to this section rather than repeating it.

The P0 plan's §4 already pre-answered two of these ("generics via monomorphization," "Tree/Graph built on classes+collections, not new primitives"); the rest are new calls made here.

| Question | Decision | Rationale |
|---|---|---|
| **Collections: compiler intrinsics (like arrays) or written in ANX itself?** | **Written in ANX**, as an auto-included "prelude" source file compiled alongside every user program — not hardcoded into the interpreter/codegen. | Arrays are compiler intrinsics because P0 needs them before classes/generics exist to build anything else out of. That constraint is gone once classes land. Writing `List`/`Stack`/`Queue`/`HashMap` in ANX itself (a) is a direct stress test of whether classes+generics are actually expressive enough for real use, not just a demo, (b) gets both the interpreter *and* codegen support for free once classes/generics work, instead of hand-writing two separate implementations (a `Value` variant plus IR lowering) per collection type, and (c) matches how mature languages actually build their standard collections (Rust's `std`, C++'s STL) on top of the language's own generics — which fits ANX's stated identity as a *real* language, not a shortcut. |
| **Import syntax for the prelude?** | **None.** The prelude is auto-parsed/compiled alongside user source, same as `runtime.c` is auto-linked at `anx build` today. `Stack<int> s = new Stack<int>();` just works with zero setup. | Directly required by the Syntax Draft's own design principle: "Built-in DSA collections as first-class types, **no standard-library import ceremony required**." A module/import system is also an explicit non-goal (PRD non-goals #4), and a single-file DSA practice problem never needs one anyway. |
| **Method dispatch** | Static only — a method call lowers to a direct function call with `this` as an implicit first argument, exactly like a free function. No vtables, no interfaces. | Interfaces/virtual dispatch are dropped from the roadmap entirely (see numbering note above) — there's no future phase this needs to stay compatible with. Nothing in the current feature list needs dynamic dispatch. |
| **Memory model for objects** | **Unchanged from P0: leak on exit.** Objects (including linked structures like `Tree`/`Graph` nodes, and now genuinely cyclic ones) are heap-allocated via the same `malloc`-and-never-free model arrays already use. | The P0 rationale (short-lived DSA scripts, small inputs) applies even more cleanly here than it did to arrays — revisit only if real dogfooding surfaces an actual problem, per the original decision's own stated revisit condition. |
| **Generics implementation** (context for P4, decided here since it affects class design) | Monomorphization, confirmed from the P0 plan's non-blocking note. A generic class instantiation (`Stack<int>`, `HashMap<string, int>`) is compiled as if it were its own concrete class per distinct type-argument combination actually used in the program. | Already decided in the P0 plan §1 with rationale (fits the no-GC, no-runtime-type-tag model better than type erasure); P4 just executes it. |

**One more decision, specific to P3 itself:** strings (P2) will likely want real method-call syntax (`s.length()`, `s.charAt(i)`) once this phase's method-call parsing exists, rather than staying free functions forever — flagged as a small follow-up to the P2 Strings Plan once this phase ships, not something P3 needs to build for.

---

## 2. Grammar additions

The syntax draft already sketches this syntax (parked, not built) — this is what actually needs parsing now. Shared across P3 (this doc) and P4 (generic type parameters build on the same productions):

```
classDecl   ::= "class" IDENTIFIER ("<" IDENTIFIER ("," IDENTIFIER)* ">")? "{" classMember* "}" ;
classMember ::= fieldDecl | ctorDecl | methodDecl ;
fieldDecl   ::= type IDENTIFIER ";" ;
ctorDecl    ::= IDENTIFIER "(" params? ")" block ;   -- name must match the class
methodDecl  ::= type IDENTIFIER "(" params? ")" block ;
type        ::= ... | IDENTIFIER ("<" type ("," type)* ">")?   -- generic instantiation, e.g. Stack<int> (P4)
newExpr     ::= "new" IDENTIFIER ("<" type ("," type)* ">")? "(" args? ")" ;
postfix     ::= ... | postfix "." IDENTIFIER "(" args? ")"     -- method call, alongside existing "." field access
primary     ::= ... | "this"
```

The generic type-parameter list (`<T>` on `classDecl`/`type`/`newExpr`) is part of the grammar sketch here for completeness, but P3 itself only needs to *parse and ignore* it if present — actually acting on it (monomorphization) is P4's job. P3's own exit gate only requires non-generic classes to work.

Notes on fit with what's already built:
- `Expr::FieldAccess` and the postfix `.` chain already exist (added in P0 for `arr.length`) — method calls are a natural extension of the same postfix-chain parsing, not new parser architecture.
- `is_lvalue` (sema) currently allows `Ident`/`Index` as assignment targets; `this.field = value;` needs `FieldAccess` added back in as a valid target — but only when the object side resolves to a user-defined class instance with a real stored field, *not* unconditionally like the pre-P0-fix version did (that regression is exactly what the P0 `arr.length = 5;` fix guarded against — don't re-open it by being generic here).
- `new ClassName(args)` reuses the existing `new` keyword, already reserved in the lexer for `new type[expr]` — needs disambiguation in the parser (`new` followed by `[` after a base type vs. `new` followed by `(` after a class name).

---

## 3. Phase plan

Get plain classes fully working end-to-end *before* generics (P4) — generics only make sense once there's a class system to parameterize.

- AST: `ClassDecl { name, fields: Vec<FieldDecl>, ctor: Option<CtorDecl>, methods: Vec<MethodDecl> }`, `Expr::New { class_name, args }`, `Expr::MethodCall { object, method, args }`, `Expr::This`.
- Sema: a type namespace alongside the existing variable/function symbol tables — class names resolve to a `ClassSig { fields: Vec<(String, Type)>, ctor_params: Vec<Type>, methods: HashMap<String, FuncSig> }`. Field access (`obj.field`) and method calls type-check against the resolved class. `this` inside a method resolves to the enclosing class's instance type.
- Interpreter: `Value::Object(Rc<RefCell<HashMap<String, Value>>>)` — same reference-semantics pattern as `Value::Array`, so passing an object into a function and mutating a field is visible to the caller, consistent with existing array behavior. Method calls become `call_function` with an extra bound `this` argument.
- Codegen: each class becomes an LLVM struct type; a method becomes a regular LLVM function taking a `this` pointer as its first parameter (static dispatch, per §1). `new ClassName(args)` mallocs the struct and calls the constructor function.
- **Exit gate:** a hand-written linked-list (`class Node { int value; Node next; }`, manual insert/traverse in `main`) runs correctly on both interpreter and compiled paths.

---

## 4. Testing strategy

Same shape as P0/P1: unit tests per layer (class AST shapes, sema resolution, interpreter object semantics, codegen IR verification + JIT correctness checks for the trickier cases). No new testing infrastructure needed — `assert_cmd`/`predicates` and the existing `tests/integration.rs` pattern extend directly.

## 5. Risks

| Risk | Mitigation |
|---|---|
| `this.field = value` reopening the exact lvalue-generality bug the P0 `arr.length` fix closed | Explicitly scoped in §2 above — `FieldAccess` is only a valid assignment target when the receiver is a class instance with a real stored field, never unconditionally. |
| Prelude-as-ANX-source approach (used by P5) turns out to need language features this phase doesn't otherwise provide (e.g., an assert/panic mechanism inside a method for a missing key) | P0's `RuntimeError` pattern (interpreter) / panic-guard pattern (codegen) already establishes how to fail cleanly at runtime — extend the same mechanism rather than inventing exceptions, since `try`/`catch` remains explicitly out of scope per the syntax draft. |
