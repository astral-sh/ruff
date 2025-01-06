# Narrowing for `!=` conditionals

## `x != None`

```py
def _(flag: bool):
    x = None if flag else 1

    if x != None:
        reveal_type(x)  # revealed: Literal[1]
    else:
        # TODO should be None
        reveal_type(x)  # revealed: None | Literal[1]
```

## `!=` for other singleton types

```py
def _(flag: bool):
    x = True if flag else False

    if x != False:
        reveal_type(x)  # revealed: Literal[True]
    else:
        # TODO should be Literal[False]
        reveal_type(x)  # revealed: bool
```

## `x != y` where `y` is of literal type

```py
def _(flag: bool):
    x = 1 if flag else 2

    if x != 1:
        reveal_type(x)  # revealed: Literal[2]
```

## `x != y` where `y` is a single-valued type

```py
def _(flag: bool):
    class A: ...
    class B: ...
    C = A if flag else B

    if C != A:
        reveal_type(C)  # revealed: Literal[B]
    else:
        # TODO should be Literal[A]
        reveal_type(C)  # revealed: Literal[A, B]
```

## `x != y` where `y` has multiple single-valued options

```py
def _(flag1: bool, flag2: bool):
    x = 1 if flag1 else 2
    y = 2 if flag2 else 3

    if x != y:
        reveal_type(x)  # revealed: Literal[1, 2]
    else:
        # TODO should be Literal[2]
        reveal_type(x)  # revealed: Literal[1, 2]
```

## `!=` for non-single-valued types

Only single-valued types should narrow the type:

```py
def _(flag: bool, a: int, y: int):
    x = a if flag else None

    if x != y:
        reveal_type(x)  # revealed: int | None
```

## Mix of single-valued and non-single-valued types

```py
def _(flag1: bool, flag2: bool, a: int):
    x = 1 if flag1 else 2
    y = 2 if flag2 else a

    if x != y:
        reveal_type(x)  # revealed: Literal[1, 2]
    else:
        reveal_type(x)  # revealed: Literal[1, 2]
```
