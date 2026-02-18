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

list(((h := i * 2) for i in Iterable()))
reveal_type(h)  # revealed: int
```

### Class body comprehension

```py
class C:
    [(x := y) for y in range(3)]
    reveal_type(x)  # revealed: int
```

### First generator `iter`

The `iter` of the first generator is evaluated in the enclosing scope. A walrus there should bind in
the enclosing scope as usual (no comprehension scope is involved).

```py
def returns_list() -> list[int]:
    return [1, 2, 3]

[x for x in (y := returns_list())]
reveal_type(y)  # revealed: list[int]
```

### Nested comprehension

```py
[[(x := y) for y in range(3)] for _ in range(3)]
reveal_type(x)  # revealed: int
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
