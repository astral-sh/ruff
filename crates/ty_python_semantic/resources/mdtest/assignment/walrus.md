# Walrus operator

## Basic

```py
x = (y := 1) + 1
reveal_type(x)  # revealed: Literal[2]
reveal_type(y)  # revealed: Literal[1]
```

## Walrus self-addition

```py
x = 0
(x := x + 1)
reveal_type(x)  # revealed: Literal[1]
```

## Walrus in comprehensions

PEP 572: Named expressions in comprehensions bind the target in the first enclosing scope that is
not a comprehension.

### List comprehension element

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

[(a := b * 2) for b in Iterable()]
reveal_type(a)  # revealed: int
```

### Comprehension filter

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

[c for d in Iterable() if (c := d - 10) > 0]
reveal_type(c)  # revealed: int
```

### Comprehension filter narrowing

```py
class State:
    state: str

def get_state(key: str) -> State | None:
    return State()

def keys() -> list[str]:
    return []

states = [state for key in keys() if (state := get_state(key)) is not None]
reveal_type(states)  # revealed: list[State]

state_names = {state.state for key in keys() if (state := get_state(key)) is not None}
reveal_type(state_names)  # revealed: set[str]

state_by_key = {key: state.state for key in keys() if (state := get_state(key)) is not None}
reveal_type(state_by_key)  # revealed: dict[str, str]
```

### Generator expression narrowing

```py
class Literal:
    fallback: str

class Proper: ...

def get_proper(item: object) -> Literal | Proper:
    return Literal()

def items() -> list[object]:
    return []

any(isinstance(p := get_proper(item), Literal) and p.fallback for item in items())
```

### Dict comprehension key captured by nested comprehension

```py
phase_sensors = {(phase_name := str(phase)): [phase_name for _ in range(1)] for phase in range(3)}
reveal_type(phase_sensors)  # revealed: dict[str, list[str]]
```

### Dict comprehension

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

{(e := f * 2): (g := f * 3) for f in Iterable()}
reveal_type(e)  # revealed: int
reveal_type(g)  # revealed: int
```

### Generator expression

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

gen = ((h := i * 2) for i in Iterable())
# error: [unresolved-reference]
reveal_type(h)  # revealed: Unknown
```

### Generator expression target is bound lazily

Named expression targets in generator expressions are not bound when the generator object is
created.

```py
x = "s"
gen = ((x := i) for i in range(3))
reveal_type(x)  # revealed: Literal["s"]

gen2 = ((y := i) for i in range(3))
# error: [unresolved-reference]
reveal_type(y)  # revealed: Unknown
```

### Generator expression target is local

Even though generator expression targets are bound lazily, they are local bindings in the enclosing
function scope.

```py
x = 0

def reads_before_generator_walrus():
    # error: [unresolved-reference]
    reveal_type(x)  # revealed: Unknown
    gen = ((x := 1) for _ in [0])

def declares_global_after_generator_walrus():
    gen = ((x := 1) for _ in [0])
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
```

### Conditional comprehension target

Named expression targets in eager comprehensions preserve the reachability of the comprehension
body.

```py
[(x := 1) for _ in [0] if False]
# error: [unresolved-reference]
reveal_type(x)  # revealed: Unknown

y = "old"
[(y := 1) for _ in [0] if False]
reveal_type(y)  # revealed: Literal["old"]
```

### Nested comprehension

```py
[[(x := y) for y in range(3)] for _ in range(3)]
reveal_type(x)  # revealed: int
```

### Named expression in later comprehension iterable

Named expressions are invalid in every comprehension iterable expression, not only the leftmost
iterable. Invalid named expressions in iterable expressions do not bind the target.

```py
[x for x in range(3) for y in (z := range(3))]  # error: [invalid-syntax]

# error: [unresolved-reference]
reveal_type(z)  # revealed: Unknown
```

### Read before named expression target is bound

Reads that execute before a comprehension named expression target is assigned do not resolve to the
target definition being created.

```py
# error: [unresolved-reference]
[(x, x := y) for y in [1]]
reveal_type(x)  # revealed: int

# error: [unresolved-reference]
[(q := q + 1) for _ in [0]]
reveal_type(q)  # revealed: Unknown
```

### Assignment diagnostics for named expression target

A named expression in a comprehension infers the enclosing-scope definition like a normal named
expression, including assignment diagnostics.

```py
x: int
[(x := "bad") for _ in range(1)]  # error: [invalid-assignment]
reveal_type(x)  # revealed: int
```

### Contextual diagnostics for named expression value

A named expression in a comprehension infers the value with the target's contextual type.

```py
from typing import Callable

f: Callable[[int], int]
[(f := lambda x: x.missing) for _ in [0]]  # error: [unresolved-attribute]
```

### Nested lazy scope captures named expression target

Nested lazy scopes capture the enclosing-scope target, not the temporary comprehension binding used
to order reads inside the comprehension.

```py
def _():
    funcs = [(x := i, lambda: x)[1] for i in range(2)]
    x = "s"
    reveal_type(funcs[0]())  # revealed: int | str
```

### Named expression target invalidates aliases

A named expression target that binds in an enclosing scope invalidates aliases in that target scope.

```py
def _(x: int | None):
    ok = x is not None
    [(x := None) for _ in range(1)]
    if ok:
        reveal_type(x)  # revealed: None
```

### Updates lazy snapshots in nested scopes

```py
def returns_str() -> str:
    return "foo"

def outer() -> None:
    x = returns_str()

    def inner() -> None:
        reveal_type(x)  # revealed: str | int
    [(x := y) for y in range(3)]
    inner()
```

### Possibly defined in `except` handlers

```py
def could_raise() -> list[int]:
    return [1]

try:
    [(y := n) for n in could_raise()]
except:
    # error: [possibly-unresolved-reference]
    reveal_type(y)  # revealed: int
```

### Honoring `global` declaration

PEP 572: the walrus honors a `global` declaration in the enclosing scope.

```py
x: int = 0

def f() -> None:
    global x
    [(x := y) for y in range(3)]
    reveal_type(x)  # revealed: int
```

### Honoring `nonlocal` declaration

PEP 572: the walrus honors a `nonlocal` declaration in the enclosing scope.

```py
def outer() -> None:
    x = "hello"

    def inner() -> None:
        nonlocal x
        [(x := y) for y in range(3)]
        reveal_type(x)  # revealed: int
    inner()
```
