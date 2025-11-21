# Unbound

## Unbound class variable

Name lookups within a class scope fall back to globals, but lookups of class attributes don't.

```py
def coinflip() -> bool:
    return True

flag = coinflip()
x = 1

class C:
    y = x
    if flag:
        x = 2

# error: [possibly-missing-attribute] "Attribute `x` may be missing on class `C`"
reveal_type(C.x)  # revealed: Unknown | Literal[2]
reveal_type(C.y)  # revealed: Unknown | Literal[1]
```

## Possibly unbound in class and global scope

```py
def coinflip() -> bool:
    return True

if coinflip():
    x = "abc"

class C:
    if coinflip():
        x = 1

    # Possibly unbound variables in enclosing scopes are considered bound.
    y = x

reveal_type(C.y)  # revealed: Unknown | Literal[1, "abc"]
```

## Possibly unbound in class scope with multiple declarations

```py
def coinflip() -> bool:
    return True

class C:
    if coinflip():
        x: int = 1
    elif coinflip():
        x: str = "abc"

# error: [possibly-missing-attribute]
reveal_type(C.x)  # revealed: int | str
```

## Unbound function local

An unbound function local that has definitions in the scope does not fall back to globals.

```py
x = 1

def f():
    # error: [unresolved-reference]
    # revealed: Unknown
    reveal_type(x)
    x = 2
    # revealed: Literal[2]
    reveal_type(x)
```
