# `assert_type`

## Basic

```py
from typing_extensions import assert_type

def _(x: int, y: bool):
    assert_type(x, int)  # fine
    # snapshot: type-assertion-failure
    assert_type(x, str)
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `str`
 --> src/mdtest_snippet.py:6:5
  |
6 |     assert_type(x, str)
  |     ^^^^^^^^^^^^-^^^^^^
  |                 |
  |                 Inferred type is `int`
  |
info: `str` and `int` are not equivalent types
```

```py
def _(x: int, y: bool):
    assert_type(assert_type(x, int), int)
    # snapshot: type-assertion-failure
    assert_type(y, int)
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `int`
  --> src/mdtest_snippet.py:10:5
   |
10 |     assert_type(y, int)
   |     ^^^^^^^^^^^^-^^^^^^
   |                 |
   |                 Inferred type is `bool`
   |
info: `bool` is a subtype of `int`, but they are not equivalent
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
    assert_type(x, int)  # error: [type-assertion-failure] "Type `bool` does not match asserted type `int`"

def _(a: type[int], b: type[Any]):
    assert_type(a, type[Any])  # error: [type-assertion-failure] "Type `type[int]` does not match asserted type `type[Any]`"
    assert_type(b, type[int])  # error: [type-assertion-failure] "Type `type[Any]` does not match asserted type `type[int]`"

# The expression constructing the type is not taken into account
def _(a: type[int]):
    assert_type(a, Type[int])  # fine
```

## Unspellable types

If the actual type is an unspellable subtype, we emit `assert-type-unspellable-subtype` instead of
`type-assertion-failure`, on the grounds that it is often useful to distinguish this from cases
where the type assertion failure is "fixable".

```py
from typing_extensions import assert_type

class Foo: ...
class Bar: ...
class Baz: ...

def f(x: Foo):
    assert_type(x, Bar)  # snapshot: type-assertion-failure
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `Bar`
 --> src/mdtest_snippet.py:8:5
  |
8 |     assert_type(x, Bar)  # snapshot: type-assertion-failure
  |     ^^^^^^^^^^^^-^^^^^^
  |                 |
  |                 Inferred type is `Foo`
  |
info: `Bar` and `Foo` are not equivalent types
```

```py
    if isinstance(x, Bar):
        assert_type(x, Bar)  # snapshot: assert-type-unspellable-subtype
```

```snapshot
error[assert-type-unspellable-subtype]: Argument does not have asserted type `Bar`
  --> src/mdtest_snippet.py:10:9
   |
10 |         assert_type(x, Bar)  # snapshot: assert-type-unspellable-subtype
   |         ^^^^^^^^^^^^-^^^^^^
   |                     |
   |                     Inferred type is `Foo & Bar`
   |
info: `Foo & Bar` is a subtype of `Bar`, but they are not equivalent
```

```py
        # The actual type must be a subtype of the asserted type, as well as being unspellable,
        # in order for `assert-type-unspellable-subtype` to be emitted instead of `type-assertion-failure`
        assert_type(x, Baz)  # snapshot: type-assertion-failure
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `Baz`
  --> src/mdtest_snippet.py:13:9
   |
13 |         assert_type(x, Baz)  # snapshot: type-assertion-failure
   |         ^^^^^^^^^^^^-^^^^^^
   |                     |
   |                     Inferred type is `Foo & Bar`
   |
info: `Baz` and `Foo & Bar` are not equivalent types
```

Compact enum complements that are equivalent to a literal union are still spellable.

```py
from enum import Flag
from typing_extensions import assert_type

class F(Flag):
    A = 1
    B = 2
    C = 4

def _(f: F):
    if f is F.A or f is F.B:
        return
    assert_type(f, F)  # error: [type-assertion-failure] "Type `Literal[F.C]` does not match asserted type `F`"
```

## Gradual types

```py
from typing import Any
from typing_extensions import Literal, assert_type

from ty_extensions import Unknown

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

from ty_extensions import Unknown

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

from ty_extensions import Intersection, Not

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
