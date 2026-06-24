## What it does

Checks for classes definitions which will fail at runtime due to
"instance memory layout conflicts".

This error is usually caused by attempting to combine multiple classes
that define non-empty `__slots__` in a class's [Method Resolution Order][method-resolution-order]
(MRO), or by attempting to combine multiple builtin classes in a class's
MRO.

## Why is this bad?

Inheriting from bases with conflicting instance memory layouts
will lead to a `TypeError` at runtime.

An instance memory layout conflict occurs when CPython cannot determine
the memory layout instances of a class should have, because the instance
memory layout of one of its bases conflicts with the instance memory layout
of one or more of its other bases.

For example, if a Python class defines non-empty `__slots__`, this will
impact the memory layout of instances of that class. Multiple inheritance
from more than one different class defining non-empty `__slots__` is not
allowed:

```python
class A:
    __slots__ = ("a", "b")


class B:
    __slots__ = ("a", "b")  # Even if the values are the same


# TypeError: multiple bases have instance lay-out conflict
class C(A, B): ...  # error
```

An instance layout conflict can also be caused by attempting to use
multiple inheritance with two builtin classes, due to the way that these
classes are implemented in a CPython C extension:

```python
# TypeError: multiple bases have instance lay-out conflict
class A(int, float): ...  # error
```

Note that pure-Python classes with no `__slots__`, or pure-Python classes
with empty `__slots__`, are always compatible:

```python
class A: ...


class B:
    __slots__ = ()


class C:
    __slots__ = ("a", "b")


# fine
class D(A, B, C): ...
```

## Known problems

Classes that have "dynamic" definitions of `__slots__` (definitions do not consist
of string literals, or tuples of string literals) are not currently considered disjoint
bases by ty.

Additionally, this check is not exhaustive: many C extensions (including several in
the standard library) define classes that use extended memory layouts and thus cannot
coexist in a single MRO. Since it is currently not possible to represent this fact in
stub files, having a full knowledge of these classes is also impossible. When it comes
to classes that do not define `__slots__` at the Python level, therefore, ty, currently
only hard-codes a number of cases where it knows that a class will produce instances with
an atypical memory layout.

## Further reading

- [CPython documentation: `__slots__`](https://docs.python.org/3/reference/datamodel.html#slots)
- [CPython documentation: Method Resolution Order](https://docs.python.org/3/glossary.html#term-method-resolution-order)

[method-resolution-order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
