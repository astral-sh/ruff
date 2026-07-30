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

A function declaration constrains later assignments by its callable signature and descriptor
behavior, not by the identity of the original function.

```py
def original(value: int) -> int:
    return value

def replacement(value: int) -> int:
    return value

original = replacement
```

## Reassigned functions expose their current identity across modules

A function that replaces another function remains a descriptor. Imports must see the replacement
function rather than preserving the original function's identity.

`implementation.py`:

```py
def original(instance: object, value: int) -> int:
    return value

def replacement(instance: object, value: int) -> int:
    return value

before = original
original = replacement
```

`main.py`:

```py
from implementation import before, original, replacement

reveal_type(original is before)  # revealed: Literal[False]
reveal_type(original is replacement)  # revealed: Literal[True]

class Container:
    method = original

reveal_type(Container().method(1))  # revealed: int
```

## Conditionally reassigned functions preserve both public identities

When reassignment depends on a condition, an imported function can still refer to either compatible
function. Its identity must not be narrowed to either possibility.

`implementation.py`:

```py
def condition() -> bool:
    return True

def original(value: int) -> int:
    return value

def replacement(value: int) -> int:
    return value

before = original

if condition():
    original = replacement
```

`main.py`:

```py
from implementation import before, original, replacement

reveal_type(original is before)  # revealed: bool
reveal_type(original is replacement)  # revealed: bool
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

## Callable objects do not preserve function descriptor behavior

A class object or callable instance cannot replace a function, even when its call signature matches.
Unlike ordinary functions, these objects do not bind an instance when assigned to a class attribute.

```py
class Factory:
    def __init__(self, value: int) -> None: ...

class CallableObject:
    def __call__(self, value: int) -> object:
        return value

def factory(value: int) -> object:
    return value

factory = Factory  # error: [invalid-assignment]

def callback(value: int) -> object:
    return value

callback = CallableObject()  # error: [invalid-assignment]
```

## Type-checking-only function declarations describe callable signatures

A function defined only under `TYPE_CHECKING` does not exist at runtime, so its declaration does not
promise function descriptor behavior. A compatible callable object can provide its implementation,
but importing that object must not incorrectly turn it into a binding descriptor.

`implementation.py`:

```py
from typing import TYPE_CHECKING

class CallableObject:
    def __call__(self, instance: object, value: int) -> int:
        return value

if TYPE_CHECKING:
    def callback(instance: object, value: int) -> int: ...

callback = CallableObject()
```

`main.py`:

```py
from implementation import callback

class Container:
    method = callback

Container().method(1)  # error: [missing-argument]
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
