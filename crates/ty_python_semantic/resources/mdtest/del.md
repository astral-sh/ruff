# `del` statement

## Basic

```py
a = 1
del a

# error: [unresolved-reference]
reveal_type(a)  # revealed: Unknown

def cond() -> bool:
    return True

b = 1
if cond():
    del b

# error: [possibly-unresolved-reference]
reveal_type(b)  # revealed: Literal[1]

c = 1
if cond():
    c = 2
else:
    del c

# error: [possibly-unresolved-reference]
reveal_type(c)  # revealed: Literal[2]

d = 1

def delete():
    # This will result in an UnboundLocalError at runtime.
    del d

delete()
reveal_type(d)  # revealed: Literal[1]

def delete_global():
    global d
    del d

delete_global()
# The variable should have been removed, but we won't track it for now.
reveal_type(d)  # revealed: Literal[1]
```

## Delete attributes

If an attribute is referenced after being deleted, it will be an error at runtime. But we don't
treat this as an error (because there may have been a redefinition by a method between the del and
the reference). However, deleting an attribute disables type narrowing by assignment, and the
attribute type will be the originally declared type.

```py
class C:
    x: int = 1

c = C()
del c.x
reveal_type(c.x)  # revealed: int

c.x = 1
reveal_type(c.x)  # revealed: Literal[1]
del c.x
reveal_type(c.x)  # revealed: int
```

## Delete items

Deleting an item also invalidates the narrowing by the assignment, but accessing the item itself is
still valid.

```py
def f(l: list[int]):
    del l[0]
    reveal_type(l[0])  # revealed: int

    l[0] = 1
    reveal_type(l[0])  # revealed: Literal[1]
    del l[0]
    reveal_type(l[0])  # revealed: int
```
