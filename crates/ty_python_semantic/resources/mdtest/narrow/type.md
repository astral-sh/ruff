# Narrowing for checks involving `type(x)`

## `type(x) is C`

```py
from typing import final

class A: ...
class B: ...

@final
class C: ...

def _(x: A | B, y: A | C):
    if type(x) is A:
        reveal_type(x)  # revealed: A
    else:
        # It would be wrong to infer `B` here. The type
        # of `x` could be a subclass of `A`, so we need
        # to infer the full union type:
        reveal_type(x)  # revealed: A | B

    if type(y) is C:
        reveal_type(y)  # revealed: C
    else:
        # here, however, inferring `A` is fine,
        # because `C` is `@final`: no subclass of `A`
        # and `C` could exist
        reveal_type(y)  # revealed: A

    if type(y) is A:
        reveal_type(y)  # revealed: A
    else:
        # but here, `type(y)` could be a subclass of `A`,
        # in which case the `type(y) is A` call would evaluate
        # to `False` even if `y` was an instance of `A`,
        # so narrowing cannot occur
        reveal_type(y)  # revealed: A | C
```

## `type(x) is not C`

```py
from typing import final

class A: ...
class B: ...

@final
class C: ...

def _(x: A | B, y: A | C):
    if type(x) is not A:
        # Same reasoning as above: no narrowing should occur here.
        reveal_type(x)  # revealed: A | B
    else:
        reveal_type(x)  # revealed: A

    if type(y) is not C:
        # same reasoning as above: narrowing *can* occur here because `C` is `@final`
        reveal_type(y)  # revealed: A
    else:
        reveal_type(y)  # revealed: C

    if type(y) is not A:
        # same reasoning as above: narrowing *cannot* occur here
        # because `A` is not `@final`
        reveal_type(y)  # revealed: A | C
    else:
        reveal_type(y)  # revealed: A
```

## No narrowing for `type(x) is C[int]`

At runtime, `type(x)` will never return a generic alias object (only ever a class-literal object),
so no narrowing can occur if `type(x)` is compared with a generic alias object.

```toml
[environment]
python-version = "3.12"
```

```py
class A[T]: ...
class B: ...

def f(x: A[int] | B):
    if type(x) is A[int]:
        # this branch is actually unreachable -- we *could* reveal `Never` here!
        reveal_type(x)  # revealed: A[int] | B
    else:
        reveal_type(x)  # revealed: A[int] | B

    if type(x) is A:
        reveal_type(x)  # revealed: A[int]
    else:
        reveal_type(x)  # revealed: A[int] | B

    if type(x) is B:
        reveal_type(x)  # revealed: B
    else:
        reveal_type(x)  # revealed: A[int] | B

    if type(x) is not A[int]:
        reveal_type(x)  # revealed: A[int] | B
    else:
        # this branch is actually unreachable -- we *could* reveal `Never` here!
        reveal_type(x)  # revealed: A[int] | B

    if type(x) is not A:
        reveal_type(x)  # revealed: A[int] | B
    else:
        reveal_type(x)  # revealed: A[int]

    if type(x) is not B:
        reveal_type(x)  # revealed: A[int] | B
    else:
        reveal_type(x)  # revealed: B
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
    # error: [invalid-argument-type] "Argument to class `type` is incorrect: Expected `str`, found `str | int`"
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

## Narrowing for generic classes

```toml
[environment]
python-version = "3.13"
```

Note that `type` returns the runtime class of an object, which does _not_ include specializations in
the case of a generic class. (The typevars are erased.) That means we cannot narrow the type to the
specialization that we compare with; we must narrow to an unknown specialization of the generic
class.

```py
class A[T = int]: ...
class B: ...

def _[T](x: A | B):
    if type(x) is A[str]:
        # TODO: `type()` never returns a generic alias, so `type(x)` cannot be `A[str]`
        reveal_type(x)  # revealed: A[int] | B
    else:
        reveal_type(x)  # revealed: A[int] | B
```

## Narrowing for tuple

An early version of <https://github.com/astral-sh/ruff/pull/19920> caused us to crash on this:

```py
def _(val):
    if type(val) is tuple:
        # TODO: better would be `Unknown & tuple[object, ...]`
        reveal_type(val)  # revealed: Unknown & tuple[Unknown, ...]
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

## Assignment expressions

```py
def _(x: object):
    if (y := type(x)) is bool:
        reveal_type(y)  # revealed: <class 'bool'>
    if (type(y := x)) is bool:
        reveal_type(y)  # revealed: bool
```
