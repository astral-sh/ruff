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

# error: [possibly-unbound-attribute] "Attribute `x` on type `Literal[C]` is possibly unbound"
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

    # error: [possibly-unresolved-reference]
    y = x

reveal_type(C.y)  # revealed: Unknown | Literal[1, "abc"]
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
