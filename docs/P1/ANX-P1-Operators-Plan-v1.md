# ANX — P1 Operators Plan (v1)

**Companion to:** [ANX-PRD-v1.md](../P0/ANX-PRD-v1.md), [ANX-Implementation-Plan-v1.md](../P0/ANX-Implementation-Plan-v1.md) (P0), [ANX-Syntax-Draft-v1.md](../P0/ANX-Syntax-Draft-v1.md) (P0), [ANX-P2-Strings-Plan-v1.md](../P2/ANX-P2-Strings-Plan-v1.md) (P2), [ANX-P3-Classes-Plan-v1.md](../P3/ANX-P3-Classes-Plan-v1.md) (P3)
**Scope:** the operator categories P0 never built — ternary, compound assignment, bitwise, and shift. Ordered before every later slice (strings, classes, generics, collections) since a complete expression grammar makes all of them more natural to write against.

## What P0 already has vs. what's actually missing

Requested categories: arithmetic, logical, unary, ternary, assignment, bitwise and shift, relational.

| Category | Status | Notes |
|---|---|---|
| Arithmetic (`+ - * / %`) | ✅ already built | No work needed. |
| Logical (`&& \|\|`) | ✅ already built, short-circuiting | No work needed. |
| Unary (`- !`) | ✅ already built | Gets one more member this round: `~` (bitwise complement). |
| Relational (`< <= > >=`) and equality (`== !=`) | ✅ already built | No work needed — assuming "rational" in the original ask was a typo for "relational"; flagging this explicitly rather than silently guessing, since if something else was actually meant, it's not covered here. |
| **Assignment** | Partial — simple `=` only | **New work: compound assignment** (`+= -= *= /= %= &= \|= ^= <<= >>=`). |
| **Ternary** | Missing entirely | **New work**: `cond ? then : else` as an expression. |
| **Bitwise** | Missing entirely | **New work**: `& \| ^ ~`. Confirmed genuinely absent by grepping the lexer's token set (also the exact gap [Dogfooding Notes](../P0/ANX-Dogfooding-Notes-v1.md) found on the "Single Number" problem). |
| **Shift** | Missing entirely | **New work**: `<< >>`. |

So the real scope is four additions: ternary, compound assignment, bitwise, shift.

---

## 1. Key design decisions

| Question | Decision | Rationale |
|---|---|---|
| Ternary AST shape | New `Expr::Ternary { cond, then_branch, else_branch }` node, not desugared to anything else. | It's a genuine expression-level construct (usable anywhere an `Expr` is, unlike `if`, which is statement-level) — there's no existing node to reuse it as. |
| Compound assignment AST shape | **A real `Expr::CompoundAssign { op: BinOp, target, value }` node — not desugared to `target = target op value` at parse time.** | Naive desugaring double-evaluates the target expression. For a simple `Ident` target that's harmless, but ANX's assignment targets can be `Index` expressions with arbitrary expressions inside the brackets (`arr[f()] += 1`) — desugaring would call `f()` twice, silently changing behavior if `f` has side effects or isn't idempotent. A dedicated node evaluates the target's "slot" (variable, or array+index) exactly once, then reads-modifies-writes through it — matching how real compilers handle this for the same reason. |
| Bitwise operand types | **`int` only** (no bitwise ops on `bool` or `float`) — matches Java/C for `float`, diverges from C's historical (ab)use of `int`-as-bool for bitwise-as-logical tricks. | ANX already has real `bool` and real `&&`/`\|\|`; there's no DSA-practice reason to allow `bool & bool` as a non-short-circuiting logical-and substitute. Keeping bitwise ops strictly `int → int → int` (and `~int → int`) avoids a confusing overlap with the logical operators. |
| Shift operand types | `int << int → int`, `int >> int → int`. Shift amount isn't range-checked against bit width (matches LLVM's own `shl`/`ashr` semantics, which are already UB-on-overflow at the IR level — not a new problem this plan introduces). | Range-checking shift amounts is real work (another runtime guard) for a case that essentially never comes up in DSA practice problems (shifting by more than 63 on an `i64`). Revisit only if dogfooding hits it. |
| Right shift: arithmetic or logical? | **Arithmetic** (sign-extending), matching Java's `>>` and C's typical (implementation-defined-but-universally-arithmetic-on-two's-complement) behavior. | ANX's only integer type is signed `int` (`i64`) — there's no unsigned type to make logical shift meaningful for, and arithmetic shift is what every DSA bit-manipulation problem actually wants (e.g. sign-preserving division-by-power-of-2 patterns). No separate `>>>` operator (Java has one for the unsigned case) since there's no unsigned type to justify it. |
| Ternary/ternary-vs-assignment precedence | Ternary binds looser than every other operator except assignment itself, and is right-associative — standard C/Java placement. | Matches what students already expect from Java; no reason to deviate. |

---

## 2. Grammar & precedence changes

Current chain (from the P0 Implementation Plan): `= (right-assoc) < || < && < ==,!= < <,<=,>,>= < +,- < */% < unary !,- < postfix`.

**New chain** (standard C/Java precedence — bitwise ops slot between logical and relational, shift slots between relational and additive):

```
assignment  ::= IDENTIFIER ("=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=") assignment
              | ternary ;
ternary     ::= logic_or ("?" expr ":" ternary)? ;      -- right-assoc, binds looser than everything below it
logic_or    ::= logic_and ("||" logic_and)* ;
logic_and   ::= bit_or ("&&" bit_or)* ;
bit_or      ::= bit_xor ("|" bit_xor)* ;
bit_xor     ::= bit_and ("^" bit_and)* ;
bit_and     ::= equality ("&" equality)* ;
equality    ::= comparison (("==" | "!=") comparison)* ;
comparison  ::= shift (("<" | "<=" | ">" | ">=") shift)* ;
shift       ::= term (("<<" | ">>") term)* ;
term        ::= factor (("+" | "-") factor)* ;
factor      ::= unary (("*" | "/" | "%") unary)* ;
unary       ::= ("!" | "-" | "~") unary | postfix ;
postfix     ::= primary (("[" expr "]") | ("." IDENTIFIER) | ("." IDENTIFIER "(" args? ")"))* ;
```

New lexer tokens needed: `Question`, `Colon`, `Amp` (`&`), `Pipe` (`|`), `Caret` (`^`), `Tilde` (`~`), `Shl` (`<<`), `Shr` (`>>`), `PlusEq`, `MinusEq`, `StarEq`, `SlashEq`, `PercentEq`, `AmpEq`, `PipeEq`, `CaretEq`, `ShlEq`, `ShrEq`. All single- or two-character, fit the existing hand-written lexer's style with no new categories of scanning logic — `<<`/`<<=` are just one more level of lookahead on top of the existing `<`/`<=` handling.

New `BinOp` variants: `BitAnd`, `BitOr`, `BitXor`, `Shl`, `Shr`. New `UnOp` variant: `BitNot`. New `Expr` variants: `Ternary { cond, then_branch, else_branch }`, `CompoundAssign { op, target, value }`.

---

## 3. Phase-by-phase plan

### Step 1 — Lexer
- Add the ~17 new tokens listed above.
- **Exit gate:** each new token lexes correctly in isolation and in combination (e.g. `x<<=2` lexes as `Ident(x)`, `ShlEq`, `IntLiteral(2)` — not `Shl`, `Eq`, or any other mis-tokenization of the 3-character operator).

### Step 2 — Parser
- Insert the four new precedence levels (`bit_or`/`bit_xor`/`bit_and` between `logic_and` and `equality`; `shift` between `comparison` and `term`), matching §2's chain.
- `ternary` sits between `assignment` and `logic_or`; parse the `?`/`:` branches with the same recursive structure as existing binary levels, but note the middle operand (`then_branch`) is parsed as a full `expr` (allowing nested ternaries/assignments there, standard C/Java behavior) while the tail is right-recursive into `ternary` itself.
- Compound assignment: extend `parse_assignment` to recognize any of the ~10 compound operators alongside plain `=`, producing `Expr::CompoundAssign` instead of `Expr::Assign`. Reuses the existing lvalue-shape check (`is_lvalue` equivalent already used for `=`) unchanged.
- **Exit gate:** every new construct parses into the expected AST shape; a precedence test confirms `a | b & c` parses as `a | (b & c)` (matching bitwise-AND-binds-tighter-than-OR) and `a << b + c` parses as `a << (b + c)` (shift binds looser than additive).

### Step 3 — Sema
- `BinOp::BitAnd/BitOr/BitXor/Shl/Shr` type-check as `(Int, Int) → Int`, alongside the existing arithmetic rules in the same match arm structure.
- `UnOp::BitNot` type-checks as `Int → Int`.
- `Expr::Ternary`: `cond` must be `Bool`; `then_branch`/`else_branch` must have the same type (no implicit coercion, matching P0's existing no-coercion stance) — that shared type is the ternary's own type.
- `Expr::CompoundAssign`: target must be a valid lvalue (same check as plain assignment); target and value types are checked against the corresponding `BinOp`'s rule (e.g. `+=` reuses the `Add` rule); result type is the target's type.
- **Exit gate:** a battery of valid/invalid programs per new construct, same pattern as every prior sema phase (e.g. `bool b = true; int x = b & 1;` produces the expected type-mismatch error; `int x = true ? 1 : "a";` produces the expected then/else-type-mismatch error).

### Step 4 — Interpreter
- Bitwise/shift ops on `Value::Int` map directly to Rust's own `&`/`|`/`^`/`!`/`<<`/`>>` on `i64` — near-zero new code, same shape as how arithmetic already works.
- `Ternary` evaluates `cond`, then evaluates *only* the taken branch (short-circuiting, like `if`/`&&`/`||` already do — not both branches).
- `CompoundAssign` evaluates the target's current value once, applies the op, writes back through the same `Ident`/`Index` write path already used by `Assign` — reusing `eval_assign`'s existing per-target-kind logic rather than duplicating it.
- **Exit gate:** all new operators run correctly through `anx run` on a small hand-written test set; specifically confirm `arr[f()] += 1` calls `f()` exactly once (the concrete double-evaluation hazard from §1).

### Step 5 — Codegen
- Bitwise/shift map directly to LLVM's `and`/`or`/`xor`/`shl`/`ashr` instructions on `i64` — one `build_*` call each, no new runtime shim functions needed (unlike strings/arrays, there's no heap allocation or bounds-checking involved).
- `Ternary` lowers to the same basic-block pattern as `if`/`else` (Phase 5's existing control-flow lowering), but as an expression: both branches computed in their own blocks, a merge block with an LLVM `phi` node selecting the result — the one genuinely new codegen shape this plan introduces (P0 never needed a `phi` node since `if` was statement-only).
- `CompoundAssign` lowers to: compute the target's address once (same GEP-or-alloca-pointer logic `Assign` already has for `Ident`/`Index`), load, apply the op, store — mirroring the interpreter's approach.
- **Exit gate:** every new construct emits verifiable IR (`Module::verify()`); JIT-execute a handful (bitwise combo, a ternary, a compound-assign-in-a-loop) and assert correct results, matching the bar Phase 5 set.

### Step 6 — Dogfooding validation
- Rewrite the "Single Number" entry from the Dogfooding Notes using real XOR (`arr[i] ^ arr[j]`-folded) now that the operator exists, confirming the originally-flagged friction point is actually closed, not just theoretically addressed.
- Add 1–2 more bit-manipulation-flavored problems (e.g. counting set bits, checking if a number is a power of two via `n & (n - 1)`) as fresh log entries.
- **Exit gate:** the XOR-based single-number solution produces the same correct answer as the original O(n²) version, on both paths.

---

## 4. Testing strategy

Same shape as every prior phase: unit tests per step (lexer tokens, parser AST/precedence shapes, sema valid/invalid battery, interpreter correctness, codegen IR verification + JIT spot-checks), no new test infrastructure needed.

## 5. Risks

| Risk | Mitigation |
|---|---|
| Precedence-chain insertion breaks an existing test that hardcodes the old chain's shape | The P0 precedence tests assert specific tree shapes (e.g. `index_binds_tighter_than_comparison`) — none of them should change meaning, since the new levels are *inserted* between existing ones, not reordering anything that was already there. Run the full existing suite after Step 2, not just the new tests. |
| `phi`-node codegen for ternary is the one genuinely unfamiliar LLVM construct in this plan | Scope it tightly: get a scalar (`int`) ternary JIT-verified first before trusting it inside anything more complex (e.g. a ternary whose branches involve arrays). |
