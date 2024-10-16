# Unbound

## Maybe unbound

```py
if flag:
    y = 3
x = y
reveal_type(x)  # revealed: Unbound | Literal[3]
```

## Unbound

```py
x = foo; foo = 1
reveal_type(x)  # revealed: Unbound
```

## Unbound class variable

Class variables can reference global variables unless overridden within the class scope.

```py
x = 1
class C:
    y = x
    if flag:
        x = 2

reveal_type(C.x) # revealed: Unbound | Literal[2]
reveal_type(C.y) # revealed: Literal[1]
```
