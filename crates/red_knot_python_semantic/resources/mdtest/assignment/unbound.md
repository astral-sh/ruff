# Unbound

## Unbound

```py
x = foo
foo = 1
reveal_type(x)  # revealed: Unbound
```

## Unbound class variable

Name lookups within a class scope fall back to globals, but lookups of class attributes don't.

```py
x = 1

class C:
    y = x
    if flag:
        x = 2

reveal_type(C.x)  # revealed: Literal[2]
reveal_type(C.y)  # revealed: Literal[1]
```
