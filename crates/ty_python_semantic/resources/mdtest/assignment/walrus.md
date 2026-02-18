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
# error: [possibly-unresolved-reference]
reveal_type(a)  # revealed: int
```

### Comprehension may not iterate

```py
def items() -> list[int]:
    return []

y = "old"
[(y := item) for item in items()]
reveal_type(y)  # revealed: Literal["old"] | int

[(z := item) for item in items()]
# error: [possibly-unresolved-reference]
reveal_type(z)  # revealed: int
```

### Shadowed comprehension assignment

```py
x = 0
[(x := None, x := 1) for _ in range(1)]
x.bit_length()
reveal_type(x)  # revealed: Literal[0, 1]
```

### Comprehension iteration observes leaked assignment

A walrus in an element can affect a filter during a subsequent iteration.

```py
def repeated_items() -> list[int]:
    return []

x = 1
[(x := None) for _ in repeated_items() if x.bit_length()]  # error: [unresolved-attribute]
```

### Rejected filter assignment remains visible

```py
def possibly_none_items() -> list[int | None]:
    return []

[(filter_value := "replacement") for item in possibly_none_items() if (filter_value := item) is None]
# error: [possibly-unresolved-reference]
reveal_type(filter_value)  # revealed: int | None | Literal["replacement"]
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
# error: [possibly-unresolved-reference]
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

nested_state_names = [[state.state for _ in [0]] for key in keys() if (state := get_state(key)) is not None]
reveal_type(nested_state_names)  # revealed: list[list[str]]
```

### Comprehension filter narrowing after earlier filter

```py
class Sensor:
    is_on: bool | None

def make_sensor(key: str) -> Sensor:
    return Sensor()

def sensor_keys() -> list[str]:
    return []

def enabled(key: str) -> bool:
    return True

sensors = [sensor for key in sensor_keys() if enabled(key) if (sensor := make_sensor(key)).is_on is not None]
reveal_type(sensors)  # revealed: list[Sensor]
```

### Comprehension element short-circuit

```py
def values() -> list[int]:
    return []

def flag() -> bool:
    return True

short_circuited = [flag() and (value := item) and value for item in values()]
reveal_type(short_circuited)  # revealed: list[int]
```

### Comprehension boolean filter narrowing

```py
class Stats:
    strength: int

def stat_keys() -> list[str]:
    return []

def get_stats(key: str) -> Stats | None:
    return Stats()

stat_values = [stats.strength for key in stat_keys() if key and (stats := get_stats(key))]
reveal_type(stat_values)  # revealed: list[int]
```

### Comprehension if-expression after earlier filter

```py
def sensor_keys() -> list[str]:
    return []

def enabled(key: str) -> bool:
    return True

def check(value: object) -> bool:
    return True

if_expression_values = {
    name: original.upper() if check(original := name) else original for name in sensor_keys() if enabled(name)
}
reveal_type(if_expression_values)  # revealed: dict[str, str]
```

### Comprehension filter can bypass walrus

```py
def values() -> list[int]:
    return []

def _(flag: bool):
    # error: [possibly-unresolved-reference]
    return [x for _ in values() if flag or (x := 1)]
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
# error: [possibly-unresolved-reference]
reveal_type(e)  # revealed: int
# error: [possibly-unresolved-reference]
reveal_type(g)  # revealed: int
```

### Generator expression eager approximation

ty models generator-comprehension scopes as eager. This matches the existing comprehension model and
avoids suppressing bindings from commonly consumed generators, although a generator body is lazy at
runtime.

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

gen = ((generator_target := i * 2) for i in Iterable())
# error: [possibly-unresolved-reference]
reveal_type(generator_target)  # revealed: int

list(((h := i * 2) for i in Iterable()))
# error: [possibly-unresolved-reference]
reveal_type(h)  # revealed: int
```

### Generator expression target is local

Generator expression targets are local bindings in the enclosing function scope.

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
from typing import Literal

[(x := 1) for _ in [0] if False]
# error: [unresolved-reference]
reveal_type(x)  # revealed: Unknown

y = "old"
[(y := 1) for _ in [0] if False]
reveal_type(y)  # revealed: Literal["old"]

flag: Literal[False] = False
[(z := 1) for _ in [0] if flag]
# error: [unresolved-reference]
reveal_type(z)  # revealed: Unknown
```

### Nested comprehension

```py
[[(x := y) for y in range(3)] for _ in range(3)]
# error: [possibly-unresolved-reference]
reveal_type(x)  # revealed: int
```

### Named expression in later comprehension iterable

Named expressions are invalid in every comprehension iterable expression, not only the leftmost
iterable. Invalid named expressions in iterable expressions do not bind the target.

```py
[x for x in range(3) for y in (z := range(3))]  # error: [invalid-syntax]

# error: [unresolved-reference]
reveal_type(z)  # revealed: Unknown

[x for x in [y for y in [1] if (nested := y)]]  # error: [invalid-syntax]

# error: [unresolved-reference]
reveal_type(nested)  # revealed: Unknown
```

### Read before named expression target is bound

Reads that execute before a comprehension named expression target is assigned can resolve to the
target definition from a preceding iteration, but the binding is not available on the first
iteration.

```py
# error: [possibly-unresolved-reference]
[(x, x := y) for y in [1]]
# error: [possibly-unresolved-reference]
reveal_type(x)  # revealed: int

# error: [possibly-unresolved-reference]
[(q := q + 1) for _ in [0]]
# error: [possibly-unresolved-reference]
reveal_type(q)  # revealed: Divergent
```

### Assignment diagnostics for named expression target

A named expression in a comprehension infers the enclosing-scope definition like a normal named
expression, including assignment diagnostics.

```py
x: int
[(x := "bad") for _ in range(1)]  # error: [invalid-assignment]
# error: [possibly-unresolved-reference]
reveal_type(x)  # revealed: int
```

### Contextual diagnostics for named expression value

A named expression in a comprehension infers the value with the target's contextual type.

```py
from typing import Callable, TypedDict

f: Callable[[int], int]
[(f := lambda x: x.missing) for _ in [0]]  # error: [unresolved-attribute]

class Bar(TypedDict):
    bar: int

ordinary: Bar
(ordinary := {})  # error: [missing-typed-dict-key]

leaked: Bar
[(leaked := {}) for _ in [0]]  # error: [missing-typed-dict-key]
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

### Lambda-local walrus target shadows comprehension iterator

```py
def _():
    funcs = [(lambda: (x := "s", x)[1]) for x in range(3)]
    reveal_type(funcs[0]())  # revealed: str
```

### Named expression target invalidates aliases

A named expression target that binds in an enclosing scope invalidates aliases in that target scope.

```py
def _(x: int | None):
    ok = x is not None
    [(x := None) for _ in range(1)]
    if ok:
        reveal_type(x)  # revealed: int | None
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

### Shadowed comprehension walruses remain visible to `except`

```py
def may_raise() -> None:
    return None

try:
    [(y := 1, may_raise(), y := "s") for _ in [0]]
except:
    # error: [possibly-unresolved-reference]
    reveal_type(y)  # revealed: Literal[1, "s"]
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
        reveal_type(x)  # revealed: int | Literal["hello"]
    inner()
```
