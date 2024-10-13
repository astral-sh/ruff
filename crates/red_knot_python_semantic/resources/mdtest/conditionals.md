# Conditionals

## Expressions

### Simple if-expression

```py
x = 1 if flag else 2
reveal_type(x)  # revealed: Literal[1, 2]
```

### If-expression with walrus operator

This test verifies the behavior of an `if-else` expression using the walrus operator (`:=`) to assign values during the condition evaluation.

```py
y = 0
z = 0
x = (y := 1) if flag else (z := 2)
a = y
b = z
reveal_type(x)  # revealed: Literal[1, 2]
reveal_type(a)  # revealed: Literal[0, 1]
reveal_type(b)  # revealed: Literal[0, 2]
```

### Nested if-expression

```py
x = 1 if flag else 2 if flag2 else 3
reveal_type(x)  # revealed: Literal[1, 2, 3]
```

### None

```py
x = 1 if flag else None
reveal_type(x)  # revealed: Literal[1] | None
```

## Statements

### Simple if

```py
y = 1
y = 2

if flag:
    y = 3

x = y

reveal_type(x)  # revealed: Literal[2, 3]
```

### Simple if-elif-else

```py
y = 1
y = 2
if flag:
    y = 3
elif flag2:
    y = 4
else:
    r = y
    y = 5
    s = y
x = y
reveal_type(x)  # revealed: Literal[3, 4, 5]
reveal_type(r)  # revealed: Unbound | Literal[2]
reveal_type(s)  # revealed: Unbound | Literal[5]
```

### Single symbol across if-elif-else

```py
if flag:
    y = 1
elif flag2:
    y = 2
else:
    y = 3
reveal_type(y)  # revealed: Literal[1, 2, 3]
```

### if-elif-else without else assignment

```py
y = 0
if flag:
    y = 1
elif flag2:
    y = 2
else:
    pass
reveal_type(y)  # revealed: Literal[0, 1, 2]
```

### if-elif-else with intervening assignment

```py
y = 0
if flag:
    y = 1
    z = 3
elif flag2:
    y = 2
else:
    pass
reveal_type(y)  # revealed: Literal[0, 1, 2]
```

### Nested if statement

```py
y = 0
if flag:
    if flag2:
        y = 1
reveal_type(y)  # revealed: Literal[0, 1]
```

### if-elif without else

```py
y = 1
y = 2
if flag:
    y = 3
elif flag2:
    y = 4
x = y

reveal_type(x)  # revealed: Literal[2, 3, 4]
```

## Narrowing

### `is not None` check

This test ensures correct type narrowing with an `is not None` check.

```py
x = None if flag else 1
y = 0
if x is not None:
    y = x

reveal_type(x)  # revealed: None | Literal[1]
reveal_type(y)  # revealed: Literal[0, 1]
```

### TODO: Singleton pattern

> TODO: The correct inferred type should be `Literal[0] | None`, but due to simplification limitations, the inferred type is `Literal[0] | None | (Literal[1] & None)`.

```py
x = None if flag else 1
y = 0
match x:
    case None:
        y = x

reveal_type(y)  # revealed: Literal[0] | None | Literal[1] & None
```
