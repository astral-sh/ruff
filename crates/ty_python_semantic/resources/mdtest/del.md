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

d = [1, 2, 3]

def delete():
    del d  # error: [unresolved-reference] "Name `d` used when not defined"

delete()
reveal_type(d)  # revealed: list[Unknown]

def delete_element():
    # When the `del` target isn't a name, it doesn't force local resolution.
    del d[0]
    print(d)

def delete_global():
    global d
    del d
    # We could lint that `d` is unbound in this trivial case, but because it's global we'd need to
    # be careful about false positives if `d` got reinitialized somehow in between the two `del`s.
    del d

delete_global()
# Again, the variable should have been removed, but we don't check it.
reveal_type(d)  # revealed: list[Unknown]

def delete_nonlocal():
    e = 2

    def delete_nonlocal_bad():
        del e  # error: [unresolved-reference] "Name `e` used when not defined"

    def delete_nonlocal_ok():
        nonlocal e
        del e
        # As with `global` above, we don't track that the nonlocal `e` is unbound.
        del e
```

## `del` forces local resolution even if it's unreachable

Without a `global x` or `nonlocal x` declaration in `foo`, `del x` in `foo` causes `print(x)` in an
inner function `bar` to resolve to `foo`'s binding, in this case an unresolved reference / unbound
local error:

```py
x = 1

def foo():
    print(x)  # error: [unresolved-reference] "Name `x` used when not defined"
    if False:
        # Assigning to `x` would have the same effect here.
        del x

    def bar():
        print(x)  # error: [unresolved-reference] "Name `x` used when not defined"
```

## But `del` doesn't force local resolution of `global` or `nonlocal` variables

However, with `global x` in `foo`, `print(x)` in `bar` resolves in the global scope, despite the
`del` in `foo`:

```py
x = 1

def foo():
    global x
    def bar():
        # allowed, refers to `x` in the global scope
        reveal_type(x)  # revealed: Unknown | Literal[1]
    bar()
    del x  # allowed, deletes `x` in the global scope (though we don't track that)
```

`nonlocal x` has a similar effect, if we add an extra `enclosing` scope to give it something to
refer to:

```py
def enclosing():
    x = 2
    def foo():
        nonlocal x
        def bar():
            # allowed, refers to `x` in `enclosing`
            reveal_type(x)  # revealed: Literal[2]
        bar()
        del x  # allowed, deletes `x` in `enclosing` (though we don't track that)
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

    # error: [invalid-argument-type]
    del l["string"]

    l[0] = 1
    reveal_type(l[0])  # revealed: Literal[1]
    del l[0]
    reveal_type(l[0])  # revealed: int
```
