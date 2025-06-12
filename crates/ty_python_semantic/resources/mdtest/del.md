# `del` statement

## Basic

```py
a = 1
del a
# error: [unresolved-reference]
reveal_type(a)  # revealed: Unknown

# error: [invalid-syntax] "Invalid delete target"
del 1

# error: [unresolved-reference]
del a

x, y = 1, 2
del x, y
# error: [unresolved-reference]
reveal_type(x)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(y)  # revealed: Unknown

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
    # TODO: this results in `UnboundLocalError`; we should emit `unresolved-reference`
    del d

delete()
reveal_type(d)  # revealed: Literal[1]

def delete_global():
    global d
    del d

delete_global()
# The variable should have been removed, but we won't check it for now.
reveal_type(d)  # revealed: Literal[1]
```

## Delete attributes

If an attribute is referenced after being deleted, it will be an error at runtime. But we don't
treat this as an error (because there may have been a redefinition by a method between the `del`
statement and the reference). However, deleting an attribute invalidates type narrowing by
assignment, and the attribute type will be the originally declared type.

### Invalidate narrowing

```py
class C:
    x: int = 1

c = C()
del c.x
reveal_type(c.x)  # revealed: int

# error: [unresolved-attribute]
del c.non_existent

c.x = 1
reveal_type(c.x)  # revealed: Literal[1]
del c.x
reveal_type(c.x)  # revealed: int
```

### Delete an instance attribute definition

```py
class C:
    x: int = 1

c = C()
reveal_type(c.x)  # revealed: int

del C.x
c = C()
# This attribute is unresolved, but we won't check it for now.
reveal_type(c.x)  # revealed: int
```

## Delete items

Deleting an item also invalidates the narrowing by the assignment, but accessing the item itself is
still valid.

```py
def f(l: list[int]):
    del l[0]
    # If the length of `l` was 1, this will be a runtime error,
    # but if it was greater than that, it will not be an error.
    reveal_type(l[0])  # revealed: int

    # error: [call-non-callable]
    del l["string"]

    l[0] = 1
    reveal_type(l[0])  # revealed: Literal[1]
    del l[0]
    reveal_type(l[0])  # revealed: int
```
