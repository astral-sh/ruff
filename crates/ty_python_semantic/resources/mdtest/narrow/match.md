# Narrowing for `match` statements

```toml
[environment]
python-version = "3.10"
```

## Single `match` pattern

```py
def _(flag: bool):
    x = None if flag else 1

    reveal_type(x)  # revealed: None | Literal[1]

    y = 0

    match x:
        case None:
            y = x

    reveal_type(y)  # revealed: Literal[0] | None
```

## Class patterns

```py
def get_object() -> object:
    return object()

class A: ...
class B: ...

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case A():
        reveal_type(x)  # revealed: A
    case B():
        reveal_type(x)  # revealed: B & ~A

reveal_type(x)  # revealed: object
```

## Class pattern with guard

```py
def get_object() -> object:
    return object()

class A:
    def y() -> int:
        return 1

class B: ...

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case A() if reveal_type(x):  # revealed: A
        pass
    case B() if reveal_type(x):  # revealed: B
        pass

reveal_type(x)  # revealed: object
```

## Value patterns

Value patterns are evaluated by equality, which is overridable. Therefore successfully matching on
one can only give us information where we know how the subject type implements equality.

Consider the following example.

```py
from typing import Literal

def _(x: Literal["foo"] | int):
    match x:
        case "foo":
            reveal_type(x)  # revealed: Literal["foo"] | int

    match x:
        case "bar":
            reveal_type(x)  # revealed: int
```

In the first `match`'s `case "foo"` all we know is `x == "foo"`. `x` could be an instance of an
arbitrary `int` subclass with an arbitrary `__eq__`, so we can't actually narrow to
`Literal["foo"]`.

In the second `match`'s `case "bar"` we know `x == "bar"`. As discussed above, this isn't enough to
rule out `int`, but we know that `"foo" == "bar"` is false so we can eliminate `Literal["foo"]`.

More examples follow.

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case "foo":
        reveal_type(x)  # revealed: object
    case 42:
        reveal_type(x)  # revealed: ~Literal["foo"]
    case 6.0:
        reveal_type(x)  # revealed: ~Literal["foo"] & ~Literal[42]
    case 1j:
        reveal_type(x)  # revealed: ~Literal["foo"] & ~Literal[42]
    case b"foo":
        reveal_type(x)  # revealed: ~Literal["foo"] & ~Literal[42]
    case _:
        reveal_type(x)  # revealed: ~Literal["foo"] & ~Literal[42] & ~Literal[b"foo"]

reveal_type(x)  # revealed: object
```

## Value patterns with guard

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case "foo" if reveal_type(x):  # revealed: object
        pass
    case 42 if reveal_type(x):  # revealed: object
        pass
    case 6.0 if reveal_type(x):  # revealed: object
        pass
    case 1j if reveal_type(x):  # revealed: object
        pass
    case b"foo" if reveal_type(x):  # revealed: object
        pass

reveal_type(x)  # revealed: object
```

## Or patterns

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case "foo" | 42 | None:
        reveal_type(x)  # revealed: object
    case "foo" | tuple():
        reveal_type(x)  # revealed: ~Literal["foo"] & ~Literal[42] & ~None
    case True | False:
        reveal_type(x)  # revealed: bool
    case 3.14 | 2.718 | 1.414:
        # revealed: ~Literal["foo"] & ~Literal[42] & ~None & ~tuple[Unknown, ...] & ~Literal[True] & ~Literal[False]
        reveal_type(x)

reveal_type(x)  # revealed: object
```

## Or patterns with guard

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case "foo" | 42 | None if reveal_type(x):  # revealed: object
        pass
    case "foo" | tuple() if reveal_type(x):  # revealed: object
        pass
    case True | False if reveal_type(x):  # revealed: bool
        pass
    case 3.14 | 2.718 | 1.414 if reveal_type(x):  # revealed: object
        pass

reveal_type(x)  # revealed: object
```

## Narrowing due to guard

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case str() | float() if type(x) is str:
        reveal_type(x)  #  revealed: str
    case "foo" | 42 | None if isinstance(x, int):
        reveal_type(x)  #  revealed: int
    case False if x:
        reveal_type(x)  #  revealed: Never
    case "foo" if x := "bar":
        reveal_type(x)  # revealed: Literal["bar"]

reveal_type(x)  # revealed: object
```

## Guard and reveal_type in guard

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case str() | float() if type(x) is str and reveal_type(x):  # revealed: str
        pass
    case "foo" | 42 | None if isinstance(x, int) and reveal_type(x):  #  revealed: int
        pass
    case False if x and reveal_type(x):  #  revealed: Never
        pass
    case "foo" if (x := "bar") and reveal_type(x):  #  revealed: Literal["bar"]
        pass

reveal_type(x)  # revealed: object
```
