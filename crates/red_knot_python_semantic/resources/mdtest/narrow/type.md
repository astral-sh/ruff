# Narrowing for checks involving `type(x)`

## `type(x) is C`

```py
class A: ...
class B: ...

def _(x: A | B):
    if type(x) is A:
        reveal_type(x)  # revealed: A
    else:
        # It would be wrong to infer `B` here. The type
        # of `x` could be a subclass of `A`, so we need
        # to infer the full union type:
        reveal_type(x)  # revealed: A | B
```

## `type(x) is not C`

```py
class A: ...
class B: ...

def _(x: A | B):
    if type(x) is not A:
        # Same reasoning as above: no narrowing should occur here.
        reveal_type(x)  # revealed: A | B
    else:
        reveal_type(x)  # revealed: A
```

## `type(x) == C`, `type(x) != C`

No narrowing can occur for equality comparisons, since there might be a custom `__eq__`
implementation on the metaclass.

TODO: Narrowing might be possible in some cases where the classes themselves are `@final` or their
metaclass is `@final`.

```py
class IsEqualToEverything(type):
    def __eq__(cls, other):
        return True

class A(metaclass=IsEqualToEverything): ...
class B(metaclass=IsEqualToEverything): ...

def _(x: A | B):
    if type(x) == A:
        reveal_type(x)  # revealed: A | B

    if type(x) != A:
        reveal_type(x)  # revealed: A | B
```

## No narrowing for custom `type` callable

```py
class A: ...
class B: ...

def type(x):
    return int

def _(x: A | B):
    if type(x) is A:
        reveal_type(x)  # revealed: A | B
    else:
        reveal_type(x)  # revealed: A | B
```

## No narrowing for multiple arguments

No narrowing should occur if `type` is used to dynamically create a class:

```py
def _(x: str | int):
    # The following diagnostic is valid, since the three-argument form of `type`
    # can only be called with `str` as the first argument.
    # error: [no-matching-overload] "No overload of class `type` matches arguments"
    if type(x, (), {}) is str:
        reveal_type(x)  # revealed: str | int
    else:
        reveal_type(x)  # revealed: str | int
```

## No narrowing for keyword arguments

`type` can't be used with a keyword argument:

```py
def _(x: str | int):
    # error: [no-matching-overload] "No overload of class `type` matches arguments"
    if type(object=x) is str:
        reveal_type(x)  # revealed: str | int
```

## Narrowing if `type` is aliased

```py
class A: ...
class B: ...

def _(x: A | B):
    alias_for_type = type

    if alias_for_type(x) is A:
        reveal_type(x)  # revealed: A
```

## Limitations

```py
class Base: ...
class Derived(Base): ...

def _(x: Base):
    if type(x) is Base:
        # Ideally, this could be narrower, but there is now way to
        # express a constraint like `Base & ~ProperSubtypeOf[Base]`.
        reveal_type(x)  # revealed: Base
```
