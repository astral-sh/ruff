# Function shadowing

## Parameter

Parameter `x` of type `str` is shadowed and reassigned with a new `int` value inside the function.
No diagnostics should be generated.

```py
def f(x: str):
    x: int = int(x)
```

## Implicit error

```py
def f(): ...

f = 1  # error: "Implicit shadowing of function `f`"
```

## Explicit shadowing

```py
def f(): ...

f: int = 1
```

## Explicit shadowing involving `def` statements

Since a `def` statement is a declaration, one `def` can shadow another `def`, or shadow a previous
non-`def` declaration, without error.

```py
f = 1
reveal_type(f)  # revealed: Literal[1]

def f(): ...

reveal_type(f)  # revealed: def f() -> Unknown

def f(x: int) -> int:
    raise NotImplementedError

reveal_type(f)  # revealed: def f(x: int) -> int

f: int = 1
reveal_type(f)  # revealed: Literal[1]

def f(): ...

reveal_type(f)  # revealed: def f() -> Unknown
```
