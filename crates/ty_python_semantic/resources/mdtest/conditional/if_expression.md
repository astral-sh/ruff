# If expressions

## Simple if-expression

```py
def _(flag: bool):
    x = 1 if flag else 2
    reveal_type(x)  # revealed: Literal[1, 2]
```

## If-expression with walrus operator

```py
def _(flag: bool):
    y = 0
    z = 0
    x = (y := 1) if flag else (z := 2)
    reveal_type(x)  # revealed: Literal[1, 2]
    reveal_type(y)  # revealed: Literal[0, 1]
    reveal_type(z)  # revealed: Literal[0, 2]
```

## Nested if-expression

```py
def _(flag: bool, flag2: bool):
    x = 1 if flag else 2 if flag2 else 3
    reveal_type(x)  # revealed: Literal[1, 2, 3]
```

## None

```py
def _(flag: bool):
    x = 1 if flag else None
    reveal_type(x)  # revealed: Literal[1] | None
```

## Statically known compound conditions

Short-circuit conditions can select a single branch even when an operand has mutable truthiness.
Saving the condition's value and testing it again does not provide the same guarantee.

```py
def _(value: object):
    reveal_type(1 if value and False else 2)  # revealed: Literal[2]
    reveal_type(1 if value or True else 2)  # revealed: Literal[1]

    saved = value and False
    reveal_type(1 if saved else 2)  # revealed: Literal[1, 2]
```

A comparison chain can select a single branch even when an individual comparison returns an
arbitrary object. Saving the chain's result allows that object's truthiness to be tested again.

```py
class Comparable:
    def __lt__(self, other: int) -> object:
        return object()

def _(value: Comparable):
    reveal_type(1 if value < 1 < 0 else 2)  # revealed: Literal[2]

    saved = value < 1 < 0
    reveal_type(1 if saved else 2)  # revealed: Literal[1, 2]
```

An operand narrowed to `Never` cannot produce a result. Nested conditions preserve the remaining
short-circuit outcome when selecting a branch.

```py
def _(other: object, value: bool):
    reveal_type(1 if other and (isinstance(value, str) and value) else 2)  # revealed: Literal[2]
    reveal_type(1 if other or (not isinstance(value, str) or value) else 2)  # revealed: Literal[1]
```

## Conditions with operands equivalent to `Never`

A call whose return type is an alias of `Never` cannot produce a result. The preceding short-circuit
outcome alone selects the conditional expression's branch.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Never

type Bottom = Never

def stop() -> Bottom:
    raise RuntimeError

def _(flag: bool):
    reveal_type(1 if flag and stop() else 2)  # revealed: Literal[2]
    reveal_type(1 if flag or stop() else 2)  # revealed: Literal[1]
```

A type variable bounded by `Never` also cannot produce a result.

```py
def _[T: Never](flag: bool, value: T):
    reveal_type(1 if flag and value else 2)  # revealed: Literal[2]
```

## Condition with object that implements `__bool__` incorrectly

```py
class NotBoolable:
    __bool__: int = 3

# error: [unsupported-bool-conversion] "Boolean conversion is not supported for type `NotBoolable`"
3 if NotBoolable() else 4
```
