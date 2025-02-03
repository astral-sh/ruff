# `assert_type`

## Basic

```py
from typing_extensions import assert_type

def _(x: int):
    assert_type(x, int)  # fine
    assert_type(x, str)  # error: [type-assertion-failure]
```

## Narrowing

The asserted type is checked against the inferred type, not the declared type.

```toml
[environment]
python-version = "3.10"
```

```py
from typing_extensions import assert_type

def _(x: int | str):
    if isinstance(x, int):
        reveal_type(x)  # revealed: int
        assert_type(x, int)  # fine
```

## Equivalence

The actual type must match the asserted type precisely.

```py
from typing import Any, Type, Union
from typing_extensions import assert_type

# Subtype does not count
def _(x: bool):
    assert_type(x, int)  # error: [type-assertion-failure]

def _(a: type[int], b: type[Any]):
    assert_type(a, type[Any])  # error: [type-assertion-failure]
    assert_type(b, type[int])  # error: [type-assertion-failure]

# The expression constructing the type is not taken into account
def _(a: type[int]):
    assert_type(a, Type[int])  # fine
```

## Gradual types

```py
from typing import Any
from typing_extensions import Literal, assert_type

from knot_extensions import Unknown

# Any and Unknown are considered equivalent
def _(a: Unknown, b: Any):
    reveal_type(a)  # revealed: Unknown
    assert_type(a, Any)  # fine

    reveal_type(b)  # revealed: Any
    assert_type(b, Unknown)  # fine

def _(a: type[Unknown], b: type[Any]):
    reveal_type(a)  # revealed: type[Unknown]
    assert_type(a, type[Any])  # fine

    reveal_type(b)  # revealed: type[Any]
    assert_type(b, type[Unknown])  # fine
```

## Tuples

Tuple types with the same elements are the same.

```py
from typing_extensions import Any, assert_type

from knot_extensions import Unknown

def _(a: tuple[int, str, bytes]):
    assert_type(a, tuple[int, str, bytes])  # fine

    assert_type(a, tuple[int, str])  # error: [type-assertion-failure]
    assert_type(a, tuple[int, str, bytes, None])  # error: [type-assertion-failure]
    assert_type(a, tuple[int, bytes, str])  # error: [type-assertion-failure]

def _(a: tuple[Any, ...], b: tuple[Unknown, ...]):
    assert_type(a, tuple[Any, ...])  # fine
    assert_type(a, tuple[Unknown, ...])  # fine

    assert_type(b, tuple[Unknown, ...])  # fine
    assert_type(b, tuple[Any, ...])  # fine
```

## Unions

Unions with the same elements are the same, regardless of order.

```toml
[environment]
python-version = "3.10"
```

```py
from typing_extensions import assert_type

def _(a: str | int):
    assert_type(a, str | int)
    assert_type(a, int | str)
```

## Intersections

Intersections are the same when their positive and negative parts are respectively the same,
regardless of order.

```py
from typing_extensions import assert_type

from knot_extensions import Intersection, Not

class A: ...
class B: ...
class C: ...
class D: ...

def _(a: A):
    if isinstance(a, B) and not isinstance(a, C) and not isinstance(a, D):
        reveal_type(a)  # revealed: A & B & ~C & ~D

        assert_type(a, Intersection[A, B, Not[C], Not[D]])
        assert_type(a, Intersection[B, A, Not[D], Not[C]])
```
