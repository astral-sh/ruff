# Statically-known branches

## Always false

### If

```py
x = 1

if False:
    x = 2

reveal_type(x)  # revealed: Literal[1]
```

### Else

```py
x = 1

if True:
    pass
else:
    x = 2

reveal_type(x)  # revealed: Literal[1]
```

## Always true

### If

```py
x = 1

if True:
    x = 2
else:
    pass

reveal_type(x)  # revealed: Literal[2]
```

### Else

```py
x = 1

if False:
    pass
else:
    x = 2

reveal_type(x)  # revealed: Literal[2]
```

## Combination

```py
x = 1

if True:
    x = 2
else:
    x = 3

reveal_type(x)  # revealed: Literal[2]
```

## Nested

```py path=nested_if_in_if_true.py
x = 1

if True:
    if True:
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[2]
```

```py path=nested_if_in_if_false.py
x = 1

if True:
    if False:
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[3]
```

```py path=nested_if_in_else_true.py
x = 1

if False:
    x = 2
else:
    if True:
        x = 3
    else:
        x = 4

reveal_type(x)  # revealed: Literal[3]
```

```py path=nested_if_in_else_false.py
x = 1

if False:
    x = 2
else:
    if False:
        x = 3
    else:
        x = 4

reveal_type(x)  # revealed: Literal[4]
```
