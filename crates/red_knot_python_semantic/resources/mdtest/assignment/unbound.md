# Unbound

## Unbound

```py
x = foo  # error: [unresolved-reference] "Name `foo` used when not defined"
foo = 1

# No error `unresolved-reference` diagnostic is reported for `x`. This is
# desirable because we would get a lot of cascading errors even though there
# is only one root cause (the unbound variable `foo`).

# revealed: Unknown
reveal_type(x)
```

Note: in this particular example, one could argue that the most likely error would be a wrong order
of the `x`/`foo` definitions, and so it could be desirable to infer `Literal[1]` for the type of
`x`. On the other hand, there might be a variable `fob` a little higher up in this file, and the
actual error might have been just a typo. Inferring `Unknown` thus seems like the safest option.

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

## Possibly unbound in class and global scope

```py
def bool_instance() -> bool:
    return True

if bool_instance():
    x = "abc"

class C:
    if bool_instance():
        x = 1

    # error: [possibly-unresolved-reference]
    y = x

reveal_type(C.y)  # revealed: Literal[1] | Literal["abc"]
```
