# Tuples

## Tuples as product types

Tuples can be used to construct product types. Inhabitants of the type `tuple[P, Q]` are ordered
pairs `(p, q)` where `p` is an inhabitant of `P` and `q` is an inhabitant of `Q`, analogous to the
Cartesian product of sets.

```py
from typing_extensions import assert_type

class P: ...
class Q: ...

def _(p: P, q: Q):
    assert_type((p, q), tuple[P, Q])
```

## Subtyping relationships

The type `tuple[S1, S2]` is a subtype of `tuple[T1, T2]` if and only if `S1` is a subtype of `T1`
and `S2` is a subtype of `T2`, and similar for other lengths of tuples:

```py
from ty_extensions import static_assert, is_subtype_of

class T1: ...
class S1(T1): ...
class T2: ...
class S2(T2): ...

static_assert(is_subtype_of(tuple[S1], tuple[T1]))
static_assert(not is_subtype_of(tuple[T1], tuple[S1]))

static_assert(is_subtype_of(tuple[S1, S2], tuple[T1, T2]))
static_assert(not is_subtype_of(tuple[T1, S2], tuple[S1, T2]))
static_assert(not is_subtype_of(tuple[S1, T2], tuple[T1, S2]))
```

Different-length tuples are not related via subtyping:

```py
static_assert(not is_subtype_of(tuple[S1], tuple[T1, T2]))
```

## The empty tuple

The type of the empty tuple `()` is spelled `tuple[()]`. It is *not* a singleton type, as it can be
subclassed:

```py
from ty_extensions import static_assert, is_singleton, is_subtype_of, is_equivalent_to, is_assignable_to

static_assert(not is_singleton(tuple[()]))

class AnotherEmptyTuple(tuple[()]): ...

static_assert(not is_equivalent_to(AnotherEmptyTuple, tuple[()]))

# TODO: These should not be errors
# error: [static-assert-error]
static_assert(is_subtype_of(AnotherEmptyTuple, tuple[()]))
# error: [static-assert-error]
static_assert(is_assignable_to(AnotherEmptyTuple, tuple[()]))
```

## Singletons?

For the same reason, all other tuples are also not singletons, even if every element is a singleton:

```py
from ty_extensions import static_assert, is_singleton

static_assert(is_singleton(None))
static_assert(not is_singleton(tuple[None]))
```

## Disjointness

A tuple `tuple[P1, P2]` is disjoint from a tuple `tuple[Q1, Q2]` if either `P1` is disjoint from
`Q1` or if `P2` is disjoint from `Q2`:

```py
from ty_extensions import static_assert, is_disjoint_from
from typing import final

@final
class F1: ...

@final
class F2: ...

class N1: ...
class N2: ...

static_assert(is_disjoint_from(F1, F2))
static_assert(not is_disjoint_from(N1, N2))

static_assert(is_disjoint_from(tuple[F1, F2], tuple[F2, F1]))
static_assert(is_disjoint_from(tuple[F1, N1], tuple[F2, N2]))
static_assert(is_disjoint_from(tuple[N1, F1], tuple[N2, F2]))
static_assert(not is_disjoint_from(tuple[N1, N2], tuple[N2, N1]))
```

## Truthiness

The truthiness of the empty tuple is `False`:

```py
from typing_extensions import assert_type, Literal

assert_type(bool(()), Literal[False])
```

The truthiness of non-empty tuples is always `True`, even if all elements are falsy:

```py
from typing_extensions import assert_type, Literal

assert_type(bool((False,)), Literal[True])
assert_type(bool((False, False)), Literal[True])
```
