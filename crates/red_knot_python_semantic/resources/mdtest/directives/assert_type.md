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
    # TODO: Infer the second argument as a type expression
    assert_type(a, Type[int])  # error: [type-assertion-failure]
```

## Gradual types

```py
from typing_extensions import Literal, assert_type

# Any and Unknown are considered equivalent
def _(a):
    reveal_type(a)  # revealed: Unknown
    assert_type(a, Any)  # fine

def _(b: type[Literal]):  # TODO: Should be invalid
    # TODO: Should be `type[Unknown]`
    reveal_type(b)  # revealed: @Todo(unsupported type[X] special form)

    # TODO: Infer the second argument as a type expression
    # Should be fine
    assert_type(b, type[Any])  # error: [type-assertion-failure]
```

## Tuples

Tuple types with the same elements are the same.

```py
from typing_extensions import assert_type

def _(a: tuple[int, str, bytes]):
    # TODO: Infer the second argument as a type expression
    # Should be fine
    assert_type(a, tuple[int, str, bytes])  # error: [type-assertion-failure]

    assert_type(a, tuple[int, str])  # error: [type-assertion-failure]
    assert_type(a, tuple[int, str, bytes, None])  # error: [type-assertion-failure]
    assert_type(a, tuple[int, bytes, str])  # error: [type-assertion-failure]

def _(a: tuple[Any, ...]):
    # TODO: Infer the second argument as a type expression
    # Should be fine
    assert_type(a, tuple[Any, ...])  # error: [type-assertion-failure]
```

## Unions

Unions with the same elements are the same, regardless of order.

```py
from typing_extensions import assert_type

def _(a: str | int):
    # TODO: Infer the second argument as a type expression
    # Should be fine
    assert_type(a, str | int)  # error: [type-assertion-failure]
```

## Intersections

Intersections are the same when their positive and negative parts are respectively the same,
regardless of order.

```py
from typing_extensions import assert_type

class A: ...
class B: ...
class C: ...
class D: ...

def _(a: A):
    if isinstance(a, B) and not isinstance(a, C) and not isinstance(a, D):
        reveal_type(a)  # revealed: A & B & ~C & ~D

        # TODO: Use Python API to spell intersection type
        # assert_type(a, B & A & ~D & ~C)
```
