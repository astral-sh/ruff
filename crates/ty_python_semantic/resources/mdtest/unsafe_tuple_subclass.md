# Unsafe Tuple Subclass

We do various kinds of narrowing on tuples and tuple subclasses. For these narrowings to be sound,
we assume that tuple subclasses don't override certain dunder methods.

## `__eq__`

<!-- snapshot-diagnostics -->

```py
class A(tuple):
    # error: [unsafe-tuple-subclass]
    def __eq__(self, other):
        return False

class B(tuple):
    # error: [unsafe-tuple-subclass]
    __eq__ = None

class C(tuple):
    # error: [unsafe-tuple-subclass]
    __eq__ = lambda self, other: False
```

## `__len__`

<!-- snapshot-diagnostics -->

```py
class A(tuple):
    # error: [unsafe-tuple-subclass]
    def __len__(self):
        return 0

class B(tuple):
    # error: [unsafe-tuple-subclass]
    __len__ = None

class C(tuple):
    # error: [unsafe-tuple-subclass]
    __len__ = lambda self: 0
```

## `__bool__`

<!-- snapshot-diagnostics -->

```py
class A(tuple):
    # error: [unsafe-tuple-subclass]
    def __bool__(self):
        return False

class B(tuple):
    # error: [unsafe-tuple-subclass]
    __bool__ = None

class C(tuple):
    # error: [unsafe-tuple-subclass]
    __bool__ = lambda self: False
```
