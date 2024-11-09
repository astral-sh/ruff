# While loops

## Basic While Loop

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()
x = 1
while flag:
    x = 2

reveal_type(x)  # revealed: Literal[1, 2]
```

## While with else (no break)

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()
x = 1
while flag:
    x = 2
else:
    reveal_type(x)  # revealed: Literal[1, 2]
    x = 3

reveal_type(x)  # revealed: Literal[3]
```

## While with Else (may break)

```py
def bool_instance() -> bool:
    return True

flag, flag2 = bool_instance(), bool_instance()
x = 1
y = 0
while flag:
    x = 2
    if flag2:
        y = 4
        break
else:
    y = x
    x = 3

reveal_type(x)  # revealed: Literal[2, 3]
reveal_type(y)  # revealed: Literal[1, 2, 4]
```
