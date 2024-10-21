# Narrowing for `!=` conditionals

## `x != None`

```py
x = None if flag else 1

if x != None:
    reveal_type(x)  # revealed: Literal[1]
```

## `!=` for other singleton types

```py
x = True if flag else False

if x != False:
    reveal_type(x)  # revealed: Literal[True]
```

## `x != y` where `y` is of literal type

```py
x = 1 if flag else 2

if x != 1:
    reveal_type(x)  # revealed: Literal[2]
```

## `!=` for non-singleton types

Non-singleton types should *not* narrow the type: two instances of a
non-singleton class may occupy different addresses in memory even if
they compare equal.

```py
x = 1

if x != 0:
    reveal_type(x)  # revealed: Literal[1]
```
