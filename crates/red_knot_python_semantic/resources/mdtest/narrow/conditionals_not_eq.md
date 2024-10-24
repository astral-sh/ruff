# Narrowing for `!=` conditionals

## `x != None`

```py
x = None if flag else 1

if x != None:
    reveal_type(x)  # revealed: Literal[1]
else:
    reveal_type(x)  # revealed: None
```

## `!=` for other singleton types

```py
x = True if flag else False

if x != False:
    reveal_type(x)  # revealed: Literal[True]
else:
    reveal_type(x)  # revealed: Literal[False]
```

## `x != y` where `y` is of literal type

```py
x = 1 if flag else 2

if x != 1:
    reveal_type(x)  # revealed: Literal[2]
```

## `x != y` where `y` is a single-valued type

```py
class A: ...
class B: ...

C = A if flag else B

if C != A:
    reveal_type(C)  # revealed: Literal[B]
else:
    reveal_type(C)  # revealed: Literal[A]
```

## `x != y` where `y` has multiple single-valued options

```py
x = 1 if flag1 else 2
y = 2 if flag2 else 3

if x != y:
    reveal_type(x)  # revealed: Literal[1, 2]
else:
    reveal_type(x)  # revealed: Literal[2]
```

## `!=` for non-single-valued types

Only single-valued types should narrow the type:

```py
def int_instance() -> int: ...

x = int_instance() if flag else None
y = int_instance()

if x != y:
    reveal_type(x)  # revealed: int | None
```

## Mix of single-valued and non-single-valued types

```py
def int_instance() -> int: ...

x = 1 if flag1 else 2
y = 2 if flag2 else int_instance()

if x != y:
    reveal_type(x)  # revealed: Literal[1, 2]
else:
    reveal_type(x)  # revealed: Literal[1, 2]
```
