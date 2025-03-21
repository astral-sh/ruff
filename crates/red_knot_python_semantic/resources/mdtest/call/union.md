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
    x = f()  # error: [call-non-callable] "Object of type `Literal[1]` is not callable"
    reveal_type(x)  # revealed: Unknown | int
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
    # TODO we should mention all non-callable elements of the union
    # error: [call-non-callable] "Object of type `Literal[1]` is not callable"
    # revealed: Unknown | int
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

    x = f()  # error: [call-non-callable] "Object of type `Literal[1, "foo"]` is not callable"
    reveal_type(x)  # revealed: Unknown
```

## Mismatching signatures

Calling a union where the arguments don't match the signature of all variants.

```py
def f1(a: int) -> int:
    return a

def f2(a: str) -> str:
    return a

def _(flag: bool):
    if flag:
        f = f1
    else:
        f = f2

    # error: [invalid-argument-type] "Object of type `Literal[3]` cannot be assigned to parameter 1 (`a`) of function `f2`; expected type `str`"
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

    # error: [call-non-callable] "Object of type `Literal["This is a string literal"]` is not callable"
    x = f(3)
    reveal_type(x)  # revealed: Unknown
```

## Union of binding errors

```py
def f1(): ...
def f2(): ...
def _(flag: bool):
    if flag:
        f = f1
    else:
        f = f2

    # TODO: we should show all errors from the union, not arbitrarily pick one union element
    # error: [too-many-positional-arguments] "Too many positional arguments to function `f1`: expected 0, got 1"
    x = f(3)
    reveal_type(x)  # revealed: Unknown
```

## One not-callable, one wrong argument

```py
class C: ...

def f1(): ...
def _(flag: bool):
    if flag:
        f = f1
    else:
        f = C()

    # TODO: we should either show all union errors here, or prioritize the not-callable error
    # error: [too-many-positional-arguments] "Too many positional arguments to function `f1`: expected 0, got 1"
    x = f(3)
    reveal_type(x)  # revealed: Unknown
```

## Union including a special-cased function

```py
def _(flag: bool):
    if flag:
        f = str
    else:
        f = repr
    reveal_type(str("string"))  # revealed: Literal["string"]
    reveal_type(repr("string"))  # revealed: Literal["'string'"]
    reveal_type(f("string"))  # revealed: Literal["string", "'string'"]
```

## Cannot use an argument as both a value and a type form

```py
from knot_extensions import is_fully_static

def _(flag: bool):
    if flag:
        f = repr
    else:
        f = is_fully_static
    # error: [conflicting-argument-forms] "Argument is used as both a value and a type form in call"
    reveal_type(f(int))  # revealed: str | Literal[True]
```
