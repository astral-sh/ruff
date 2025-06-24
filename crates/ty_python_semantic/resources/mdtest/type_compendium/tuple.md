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

## Instantiating tuples

Like all classes, tuples can be instantiated by invoking the `tuple` class. When instantiating a
specialization of `tuple` we (TODO: should) check that the values passed in match the element types
defined in the specialization.

```py
# TODO: revealed: tuple[()]
reveal_type(tuple())  # revealed: tuple[Unknown, ...]
# TODO: revealed: tuple[Literal[1]]
reveal_type(tuple([1]))  # revealed: tuple[Unknown, ...]
reveal_type(tuple[int]([1]))  # revealed: tuple[int]
# TODO: error for invalid arguments
reveal_type(tuple[int, str]([1]))  # revealed: tuple[int, str]

reveal_type(().__class__())  # revealed: tuple[()]
# TODO: error for invalid arguments
reveal_type((1,).__class__())  # revealed: tuple[Literal[1]]
# TODO: error for invalid arguments
reveal_type((1, 2).__class__())  # revealed: tuple[Literal[1], Literal[2]]
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

The type of the empty tuple `()` is spelled `tuple[()]`. It is [not a singleton type], because
different instances of `()` are not guaranteed to be the same object (even if this is the case in
CPython at the time of writing).

The empty tuple can also be subclassed (further clarifying that it is not a singleton type):

```py
from ty_extensions import static_assert, is_singleton, is_subtype_of, is_equivalent_to, is_assignable_to

static_assert(not is_singleton(tuple[()]))

class AnotherEmptyTuple(tuple[()]): ...

static_assert(not is_equivalent_to(AnotherEmptyTuple, tuple[()]))

static_assert(is_subtype_of(AnotherEmptyTuple, tuple[()]))
static_assert(is_assignable_to(AnotherEmptyTuple, tuple[()]))
```

## Non-empty tuples

For the same reason as above (two instances of a tuple with the same elements might not be the same
object), non-empty tuples are also not singleton types — even if all their elements are singletons:

```py
from ty_extensions import static_assert, is_singleton

static_assert(is_singleton(None))

static_assert(not is_singleton(tuple[None]))
```

## Tuples containing `Never`

```toml
[environment]
python-version = "3.11"
```

The `Never` type contains no inhabitants, so a tuple type that contains `Never` as a mandatory
element also contains no inhabitants.

```py
from typing import Never
from ty_extensions import static_assert, is_equivalent_to

static_assert(is_equivalent_to(tuple[Never], Never))
static_assert(is_equivalent_to(tuple[int, Never], Never))
static_assert(is_equivalent_to(tuple[Never, *tuple[int, ...]], Never))
```

If the variable-length portion of a tuple is `Never`, then that portion of the tuple must always be
empty. This means that the tuple is not actually variable-length!

```py
from typing import Never
from ty_extensions import static_assert, is_equivalent_to

static_assert(is_equivalent_to(tuple[Never, ...], tuple[()]))
static_assert(is_equivalent_to(tuple[int, *tuple[Never, ...]], tuple[int]))
static_assert(is_equivalent_to(tuple[int, *tuple[Never, ...], int], tuple[int, int]))
static_assert(is_equivalent_to(tuple[*tuple[Never, ...], int], tuple[int]))
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

We currently model tuple types to *not* be disjoint from arbitrary instance types, because we allow
for the possibility of `tuple` to be subclassed

```py
class C: ...

static_assert(not is_disjoint_from(tuple[int, str], C))

class CommonSubtype(tuple[int, str], C): ...
```

Note: This is inconsistent with the fact that we model heterogeneous tuples to be disjoint from
other heterogeneous tuples above:

```py
class I1(tuple[F1, F2]): ...
class I2(tuple[F2, F1]): ...

# TODO
# This is a subtype of both `tuple[F1, F2]` and `tuple[F2, F1]`, so those two heterogeneous tuples
# should not be disjoint from each other (see conflicting test above).
class CommonSubtypeOfTuples(I1, I2): ...
```

## Truthiness

The truthiness of the empty tuple is `False`.

```py
from typing_extensions import assert_type, Literal
from ty_extensions import static_assert, is_assignable_to, AlwaysFalsy

assert_type(bool(()), Literal[False])

static_assert(is_assignable_to(tuple[()], AlwaysFalsy))
```

The truthiness of non-empty tuples is always `True`. This is true even if all elements are falsy,
and even if any element is gradual, since the truthiness of a tuple depends only on its length, not
its content.

```py
from typing_extensions import assert_type, Any, Literal
from ty_extensions import static_assert, is_assignable_to, AlwaysTruthy

assert_type(bool((False,)), Literal[True])
assert_type(bool((False, False)), Literal[True])

static_assert(is_assignable_to(tuple[Any], AlwaysTruthy))
static_assert(is_assignable_to(tuple[Any, Any], AlwaysTruthy))
static_assert(is_assignable_to(tuple[bool], AlwaysTruthy))
static_assert(is_assignable_to(tuple[bool, bool], AlwaysTruthy))
static_assert(is_assignable_to(tuple[Literal[False]], AlwaysTruthy))
static_assert(is_assignable_to(tuple[Literal[False], Literal[False]], AlwaysTruthy))
```

The truthiness of variable-length tuples is ambiguous, since that type contains both empty and
non-empty tuples.

```py
from typing_extensions import Any, Literal
from ty_extensions import static_assert, is_assignable_to, AlwaysFalsy, AlwaysTruthy

static_assert(not is_assignable_to(tuple[Any, ...], AlwaysFalsy))
static_assert(not is_assignable_to(tuple[Any, ...], AlwaysTruthy))
static_assert(not is_assignable_to(tuple[bool, ...], AlwaysFalsy))
static_assert(not is_assignable_to(tuple[bool, ...], AlwaysTruthy))
static_assert(not is_assignable_to(tuple[Literal[False], ...], AlwaysFalsy))
static_assert(not is_assignable_to(tuple[Literal[False], ...], AlwaysTruthy))
static_assert(not is_assignable_to(tuple[Literal[True], ...], AlwaysFalsy))
static_assert(not is_assignable_to(tuple[Literal[True], ...], AlwaysTruthy))
```

Both of these results are conflicting with the fact that tuples can be subclassed, and that we
currently allow subclasses of `tuple` to overwrite `__bool__` (or `__len__`):

```py
class NotAlwaysTruthyTuple(tuple[int]):
    def __bool__(self) -> bool:
        return False

t: tuple[int] = NotAlwaysTruthyTuple((1,))
```

[not a singleton type]: https://discuss.python.org/t/should-we-specify-in-the-language-reference-that-the-empty-tuple-is-a-singleton/67957
