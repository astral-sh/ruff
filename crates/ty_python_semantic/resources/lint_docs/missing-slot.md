## What it does

Checks for assignments to declared attributes that have no matching `__slots__` entry on the class
or its bases, and no instance dictionary to store their values.

## Why is this bad?

Most Python objects store their attributes in an "instance dictionary". Assigning to a new
attribute adds an entry to this dictionary; deleting that attribute removes it again. Accordingly,
most Python objects allow for **arbitrary attributes to be set and read**. The advantage of this is
that it allows for many dynamic features; the disadvantage is that it can be costly in terms of
memory, and can easily allow for typos to slip in accidentally, e.g.:

```py
class Foo:
    def __init__(self, x):
        self.x = x

    def update_x(self, x):
        self.xx = x  # oops, this was meant to be the same attribute set in `__init__`,
        # but ended up being an entirely separate one!
```

Defining `__slots__` lets a class reserve space for a fixed set of instance attributes instead.
Unless an instance dictionary is inherited from a base class or requested by including `"__dict__"`
in `__slots__`, instances of the class have no dictionary in which to store additional attributes.
Attempting to assign to an attribute not declared in `__slots__` will often raise `AttributeError`
at runtime if the instance has no instance dictionary.

## Examples

### Class definitions

```python
class Item:
    __slots__ = ()
    value: int


Item().value = 1  # error: [missing-slot]
```

If you control the class, include the attribute in `__slots__` to make the assignment valid:

```python
class Item:
    __slots__ = ("value",)
    value: int


Item().value = 1
```

### Stub files

Stub files can use properties to indicate that instances have attributes that are readable and
writable but do not appear in `__slots__`, for example:

```pyi
class Item:
    __slots__ = ()
    @property
    def value(self) -> int: ...
    @value.setter
    def value(self, value: int) -> None: ...
```

## References

- [Python data model: `__slots__`](https://docs.python.org/3/reference/datamodel.html#slots)
