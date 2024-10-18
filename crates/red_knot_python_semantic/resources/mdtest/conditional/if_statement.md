# If statements

## Simple if

```py
y = 1
y = 2

if flag:
    y = 3

reveal_type(y)  # revealed: Literal[2, 3]
```

## Simple if-elif-else

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

## Single symbol across if-elif-else

```py
if flag:
    y = 1
elif flag2:
    y = 2
else:
    y = 3
reveal_type(y)  # revealed: Literal[1, 2, 3]
```

## if-elif-else without else assignment

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

## if-elif-else with intervening assignment

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

## Nested if statement

```py
y = 0
if flag:
    if flag2:
        y = 1
reveal_type(y)  # revealed: Literal[0, 1]
```

## if-elif without else

```py
y = 1
y = 2
if flag:
    y = 3
elif flag2:
    y = 4

reveal_type(y)  # revealed: Literal[2, 3, 4]
```
