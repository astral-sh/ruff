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

f = 1  # error: [invalid-assignment]
```

## Compatible function reassignment

A function declaration constrains later assignments by its callable signature, not by the identity
of the original function.

```py
def original(value: int) -> int:
    return value

def replacement(value: int) -> int:
    return value

original = replacement
```

## Imported compatible function reassignment

An imported function with the same signature can replace an existing function declaration.

`implementation.py`:

```py
def imported(value: int) -> int:
    return value
```

`main.py`:

```py
def imported(value: int) -> int:
    return value

from implementation import imported
```

## Compatible callable-class reassignment

Class objects are also compatible when their constructor satisfies the declared callable signature.

```py
class Factory:
    def __init__(self, value: int) -> None: ...

def factory(value: int) -> object:
    return value

factory = Factory
```

## Incompatible function reassignment

A replacement function must accept the original function's arguments and return a compatible type.

```py
def original(value: int) -> int:
    return value

def different_type(value: str) -> str:
    return value

original = different_type  # error: [invalid-assignment]
```

Parameter names remain significant when callers may pass them by keyword.

```py
def accepts_value(value: int) -> int:
    return value

def accepts_number(number: int) -> int:
    return number

accepts_value = accepts_number  # error: [invalid-assignment]
```

## Class methods preserve descriptor behavior

A callable instance does not bind an instance argument when accessed as a class attribute, so it
cannot replace a function-like method even when their call signatures otherwise match.

```py
class CallableObject:
    def __call__(this, self: object, value: int) -> int:
        return value

class Container:
    def method(self, value: int) -> int:
        return value

    method = CallableObject()  # error: [invalid-assignment]
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
