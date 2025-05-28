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

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case "foo":
        reveal_type(x)  # revealed: Literal["foo"]
    case 42:
        reveal_type(x)  # revealed: Literal[42]
    case 6.0:
        reveal_type(x)  # revealed: float
    case 1j:
        reveal_type(x)  # revealed: complex & ~float
    case b"foo":
        reveal_type(x)  # revealed: Literal[b"foo"]

reveal_type(x)  # revealed: object
```

## Value patterns with guard

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case "foo" if reveal_type(x):  # revealed: Literal["foo"]
        pass
    case 42 if reveal_type(x):  # revealed: Literal[42]
        pass
    case 6.0 if reveal_type(x):  # revealed: float
        pass
    case 1j if reveal_type(x):  # revealed: complex
        pass
    case b"foo" if reveal_type(x):  # revealed: Literal[b"foo"]
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
        reveal_type(x)  # revealed: Literal["foo", 42] | None
    case "foo" | tuple():
        reveal_type(x)  # revealed: tuple[Unknown, ...]
    case True | False:
        reveal_type(x)  # revealed: bool
    case 3.14 | 2.718 | 1.414:
        reveal_type(x)  # revealed: float & ~tuple[Unknown, ...]

reveal_type(x)  # revealed: object
```

## Or patterns with guard

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case "foo" | 42 | None if reveal_type(x):  # revealed: Literal["foo", 42] | None
        pass
    case "foo" | tuple() if reveal_type(x):  # revealed: Literal["foo"] | tuple[Unknown, ...]
        pass
    case True | False if reveal_type(x):  # revealed: bool
        pass
    case 3.14 | 2.718 | 1.414 if reveal_type(x):  # revealed: float
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
        reveal_type(x)  #  revealed: Literal[42]
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
    case "foo" | 42 | None if isinstance(x, int) and reveal_type(x):  #  revealed: Literal[42]
        pass
    case False if x and reveal_type(x):  #  revealed: Never
        pass
    case "foo" if (x := "bar") and reveal_type(x):  #  revealed: Literal["bar"]
        pass

reveal_type(x)  # revealed: object
```
