# Unbound

## Unbound

```py
x = foo  # error: [unresolved-reference] "Name `foo` used when not defined"
foo = 1

# error: [unresolved-reference]
# revealed: Unbound
reveal_type(x)
```

## Unbound class variable

Name lookups within a class scope fall back to globals, but lookups of class attributes don't.

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()
x = 1

class C:
    y = x
    if flag:
        x = 2

reveal_type(C.x)  # revealed: Literal[2]
reveal_type(C.y)  # revealed: Literal[1]
```
