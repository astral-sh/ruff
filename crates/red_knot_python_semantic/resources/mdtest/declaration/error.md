# Errors while declaring

## Violates previous assignment

```py
x = 1
x: str  # error: [invalid-declaration] "Cannot declare type `str` for inferred type `Literal[1]`"
```

## Incompatible declarations

```py
def _(flag: bool):
    if flag:
        x: str
    else:
        x: int

    x = 1  # error: [conflicting-declarations] "Conflicting declared types for `x`: str, int"
```

## Incompatible declarations for 2 (out of 3) types

```py
def _(flag1: bool, flag2: bool):
    if flag1:
        x: str
    elif flag2:
        x: int

    # Here, the declared type for `x` is `int | str | Unknown`.
    x = 1  # error: [conflicting-declarations] "Conflicting declared types for `x`: str, int"
```

## Incompatible declarations with bad assignment

```py
def _(flag: bool):
    if flag:
        x: str
    else:
        x: int

    # error: [conflicting-declarations]
    # error: [invalid-assignment]
    x = b"foo"
```

## No errors

Currently, we avoid raising the conflicting-declarations for the following cases:

### Partial declarations

```py
def _(flag: bool):
    if flag:
        x: int

    x = 1
```

### Partial declarations in try-except

Refer to <https://github.com/astral-sh/ruff/issues/13966>

```py
def _():
    try:
        x: int = 1
    except:
        x = 2

    x = 3
```
