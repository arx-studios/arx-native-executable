# ANX — Syntax Draft (v3): standard keywords

*ANX: Arx Native eXecutable*

## Design principles

- Java-like syntax, carried forward from the original ANX — familiar to the primary audience of DSA learners, and simplest to actually build the lexer/parser against.
- Statically typed, curly-brace, semicolon-terminated — real language semantics, not simplified pseudocode.
- Built-in DSA collections as first-class types, no standard-library import ceremony required.
- Cyberpunk theming is parked, not scrapped: keywords are just a token-to-string table in the lexer, so a themed alias layer could be added later without touching the core grammar, once the compiler actually works.

## Core syntax

### Variables & primitives
```
int x = 5;
float pi = 3.14;
bool found = false;
string name = "arxcy";
```

### Arrays
```
int[] nums = [1, 2, 3, 4, 5];
print(nums[0]);
```

### Functions & recursion
```
int add(int a, int b) {
    return a + b;
}

int fib(int n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}
```

### Control flow
```
if (x > 0) {
    print("positive");
} else if (x == 0) {
    print("zero");
} else {
    print("negative");
}

for (int i = 0; i < nums.length; i = i + 1) {
    print(nums[i]);
}

while (x > 0) {
    x = x - 1;
}
```

### Built-in collections (generics) — deferred, not part of the initial build
```
Stack<int> stack = new Stack<int>();
stack.push(1);
stack.push(2);
int top = stack.pop();

Queue<int> queue = new Queue<int>();
queue.enqueue(10);
int front = queue.dequeue();

HashMap<string, int> map = new HashMap<string, int>();
map.put("a", 1);
int val = map.get("a");

List<int> list = new List<int>();
list.add(5);
```

### Classes — deferred, not part of the initial build (sketched now for consistency)
```
class Node {
    int value;
    Node next;

    Node(int value) {
        this.value = value;
        this.next = null;
    }
}

class LinkedList {
    void insert(int value) {
        Node n = new Node(value);
        n.next = this.head;
        this.head = n;
    }

    Node head;
}
```

### Interfaces (P2 sketch — not implemented in v1)
```
interface Comparable {
    int compareTo(Object other);
}
```

### Error handling
```
try {
    riskyOperation();
} catch (Exception e) {
    print("caught: " + e.message);
}

void riskyOperation() {
    if (somethingBadHappened) {
        throw new Exception("bad input");
    }
}
```

## Worked example: binary search
```
int binarySearch(int[] arr, int target) {
    int lo = 0;
    int hi = arr.length - 1;
    while (lo <= hi) {
        int mid = lo + (hi - lo) / 2;
        if (arr[mid] == target) return mid;
        else if (arr[mid] < target) lo = mid + 1;
        else hi = mid - 1;
    }
    return -1;
}
```

## Grammar sketch (P0 subset, informal EBNF)
```
program     ::= declaration* ;
declaration ::= varDecl | funcDecl | classDecl ;
varDecl     ::= type IDENTIFIER ("=" expr)? ";" ;
funcDecl    ::= type IDENTIFIER "(" params? ")" block ;
type        ::= "int" | "float" | "bool" | "string" | type "[]" | IDENTIFIER ("<" type ">")? ;
statement   ::= exprStmt | ifStmt | whileStmt | forStmt | returnStmt | block ;
ifStmt      ::= "if" "(" expr ")" statement ("else" statement)? ;
whileStmt   ::= "while" "(" expr ")" statement ;
forStmt     ::= "for" "(" varDecl expr ";" expr ")" statement ;
expr        ::= assignment ;
assignment  ::= IDENTIFIER "=" assignment | logic_or ;
```

## Open questions this draft doesn't resolve

- **Null handling** — does ANX have `null` (Java-style, with the classic null-pointer risk), or force explicit optionals?
- **Type inference** — does a `var` keyword exist, or is every declaration explicitly typed?
- **String semantics** — mutable or immutable, like Java's `String`?
