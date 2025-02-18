# Unions in calls

## Union of return types

```py
def _(flag: bool):
    if flag:
        def f() -> int:
            return 1
    else:
        def f() -> str:
            return "foo"
    reveal_type(f())  # revealed: int | str
```

## Calling with an unknown union

```py
from nonexistent import f  # error: [unresolved-import] "Cannot resolve import `nonexistent`"

def coinflip() -> bool:
    return True

if coinflip():
    def f() -> int:
        return 1

reveal_type(f())  # revealed: Unknown | int
```

## Non-callable elements in a union

Calling a union with a non-callable element should emit a diagnostic.

```py
def _(flag: bool):
    if flag:
        f = 1
    else:
        def f() -> int:
            return 1
    x = f()  # error: "Object of type `Literal[1] | Literal[f]` is not callable (due to union element `Literal[1]`)"
    reveal_type(x)  # revealed: int | Unknown
```

## Multiple non-callable elements in a union

Calling a union with multiple non-callable elements should mention all of them in the diagnostic.

```py
def _(flag: bool, flag2: bool):
    if flag:
        f = 1
    elif flag2:
        f = "foo"
    else:
        def f() -> int:
            return 1
    # error: "Object of type `Literal[1, "foo"] | Literal[f]` is not callable (due to union elements Literal[1], Literal["foo"])"
    # revealed: int | Unknown
    reveal_type(f())
```

## All non-callable union elements

Calling a union with no callable elements can emit a simpler diagnostic.

```py
def _(flag: bool):
    if flag:
        f = 1
    else:
        f = "foo"

    x = f()  # error: "Object of type `Literal[1, "foo"]` is not callable"
    reveal_type(x)  # revealed: Unknown
```

## Mismatching signatures

Calling a union where the arguments don't match the signature of all variants.

```py
def f1(a: int) -> int: ...
def f2(a: str) -> str: ...
def _(flag: bool):
    if flag:
        f = f1
    else:
        f = f2

    # error: [call-non-callable] "Object of type `Literal[f1, f2]` is not callable (due to union element `Literal[f2]`)"
    x = f(3)
    reveal_type(x)  # revealed: int | str
```

## Any non-callable variant

```py
def f1(a: int): ...
def _(flag: bool):
    if flag:
        f = f1
    else:
        f = "This is a string literal"

    # error: [call-non-callable] "Object of type `Literal[f1] | Literal["This is a string literal"]` is not callable (due to union element `Literal["This is a string literal"]`)"
    x = f(3)
    reveal_type(x)  # revealed: Unknown
```
