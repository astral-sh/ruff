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
from typing_extensions import Iterable, Never

reveal_type(tuple())  # revealed: tuple[()]
reveal_type(tuple[int]((1,)))  # revealed: tuple[int]
reveal_type(().__class__())  # revealed: tuple[()]
reveal_type((1, 2).__class__((1, 2)))  # revealed: tuple[Literal[1], Literal[2]]

def f(x: Iterable[int], y: list[str], z: Never, aa: list[Never]):
    reveal_type(tuple(x))  # revealed: tuple[int, ...]
    reveal_type(tuple(y))  # revealed: tuple[str, ...]
    reveal_type(tuple(z))  # revealed: tuple[Unknown, ...]

    # This is correct as the only inhabitants of `list[Never]` can be empty lists
    reveal_type(tuple(aa))  # revealed: tuple[()]

reveal_type(tuple((1, 2)))  # revealed: tuple[Literal[1], Literal[2]]

# TODO: should be `tuple[Literal[1], ...]`
reveal_type(tuple([1]))  # revealed: tuple[Unknown, ...]

# error: [invalid-argument-type] "Argument is incorrect: Expected `tuple[int]`, found `list[Unknown]`"
reveal_type(tuple[int]([1]))  # revealed: tuple[int]

# error: [invalid-argument-type] "Argument is incorrect: Expected `tuple[int, str]`, found `tuple[Literal[1]]`"
reveal_type(tuple[int, str]((1,)))  # revealed: tuple[int, str]

# error: [missing-argument] "No argument provided for required parameter `iterable`"
reveal_type((1,).__class__())  # revealed: tuple[Literal[1]]

# error: [missing-argument] "No argument provided for required parameter `iterable`"
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

## Homogeneous non-empty tuples

```toml
[environment]
python-version = "3.11"
```

A homogeneous tuple can contain zero or more elements of a particular type. You can represent a
tuple that can contain _one_ or more elements of that type (or any other number of minimum elements)
using a mixed tuple.

```py
def takes_zero_or_more(t: tuple[int, ...]) -> None: ...
def takes_one_or_more(t: tuple[int, *tuple[int, ...]]) -> None: ...
def takes_two_or_more(t: tuple[int, int, *tuple[int, ...]]) -> None: ...

takes_zero_or_more(())
takes_zero_or_more((1,))
takes_zero_or_more((1, 2))

takes_one_or_more(())  # error: [invalid-argument-type]
takes_one_or_more((1,))
takes_one_or_more((1, 2))

takes_two_or_more(())  # error: [invalid-argument-type]
takes_two_or_more((1,))  # error: [invalid-argument-type]
takes_two_or_more((1, 2))
```

The required elements can also appear in the suffix of the mixed tuple type.

```py
def takes_one_or_more_suffix(t: tuple[*tuple[int, ...], int]) -> None: ...
def takes_two_or_more_suffix(t: tuple[*tuple[int, ...], int, int]) -> None: ...
def takes_two_or_more_mixed(t: tuple[int, *tuple[int, ...], int]) -> None: ...

takes_one_or_more_suffix(())  # error: [invalid-argument-type]
takes_one_or_more_suffix((1,))
takes_one_or_more_suffix((1, 2))

takes_two_or_more_suffix(())  # error: [invalid-argument-type]
takes_two_or_more_suffix((1,))  # error: [invalid-argument-type]
takes_two_or_more_suffix((1, 2))

takes_two_or_more_mixed(())  # error: [invalid-argument-type]
takes_two_or_more_mixed((1,))  # error: [invalid-argument-type]
takes_two_or_more_mixed((1, 2))
```

The tuple types are equivalent regardless of whether the required elements appear in the prefix or
suffix.

```py
from ty_extensions import static_assert, is_subtype_of, is_equivalent_to

static_assert(is_equivalent_to(tuple[int, *tuple[int, ...]], tuple[*tuple[int, ...], int]))

static_assert(is_equivalent_to(tuple[int, int, *tuple[int, ...]], tuple[*tuple[int, ...], int, int]))
static_assert(is_equivalent_to(tuple[int, int, *tuple[int, ...]], tuple[int, *tuple[int, ...], int]))
```

This is true when the prefix/suffix and variable-length types are equivalent, not just identical.

```py
from ty_extensions import static_assert, is_subtype_of, is_equivalent_to

static_assert(is_equivalent_to(tuple[int | str, *tuple[str | int, ...]], tuple[*tuple[str | int, ...], int | str]))

static_assert(
    is_equivalent_to(tuple[int | str, str | int, *tuple[str | int, ...]], tuple[*tuple[int | str, ...], str | int, int | str])
)
static_assert(
    is_equivalent_to(tuple[int | str, str | int, *tuple[str | int, ...]], tuple[str | int, *tuple[int | str, ...], int | str])
)
```

## Disjointness

```toml
[environment]
python-version = "3.11"
```

Two tuples with incompatible minimum lengths are always disjoint, regardless of their element types.
(The lengths are incompatible if the minimum length of one tuple is larger than the maximum length
of the other.)

```py
from ty_extensions import static_assert, is_disjoint_from

static_assert(is_disjoint_from(tuple[()], tuple[int]))
static_assert(not is_disjoint_from(tuple[()], tuple[int, ...]))
static_assert(not is_disjoint_from(tuple[int], tuple[int, ...]))
static_assert(not is_disjoint_from(tuple[str, ...], tuple[int, ...]))
```

A tuple that is required to contain elements `P1, P2` is disjoint from a tuple that is required to
contain elements `Q1, Q2` if either `P1` is disjoint from `Q1` or if `P2` is disjoint from `Q2`.

```py
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

static_assert(is_disjoint_from(tuple[F1, *tuple[int, ...], F2], tuple[F2, *tuple[int, ...], F1]))
static_assert(is_disjoint_from(tuple[F1, *tuple[int, ...], N1], tuple[F2, *tuple[int, ...], N2]))
static_assert(is_disjoint_from(tuple[N1, *tuple[int, ...], F1], tuple[N2, *tuple[int, ...], F2]))
static_assert(not is_disjoint_from(tuple[N1, *tuple[int, ...], N2], tuple[N2, *tuple[int, ...], N1]))

static_assert(not is_disjoint_from(tuple[F1, F2, *tuple[object, ...]], tuple[*tuple[object, ...], F2, F1]))
static_assert(not is_disjoint_from(tuple[F1, N1, *tuple[object, ...]], tuple[*tuple[object, ...], F2, N2]))
static_assert(not is_disjoint_from(tuple[N1, F1, *tuple[object, ...]], tuple[*tuple[object, ...], N2, F2]))
static_assert(not is_disjoint_from(tuple[N1, N2, *tuple[object, ...]], tuple[*tuple[object, ...], N2, N1]))
```

The variable-length portion of a tuple can never cause the tuples to be disjoint, since all
variable-length tuple types contain the empty tuple. (Note that per above, the variable-length
portion of a tuple cannot be `Never`; internally we simplify this to a fixed-length tuple.)

```py
static_assert(not is_disjoint_from(tuple[F1, ...], tuple[F2, ...]))
static_assert(not is_disjoint_from(tuple[N1, ...], tuple[N2, ...]))
```

We currently model tuple types to _not_ be disjoint from arbitrary instance types, because we allow
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

```toml
[environment]
python-version = "3.11"
```

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

static_assert(is_assignable_to(tuple[int, *tuple[Any, ...]], AlwaysTruthy))
static_assert(is_assignable_to(tuple[int, *tuple[bool, ...]], AlwaysTruthy))
static_assert(is_assignable_to(tuple[int, *tuple[Literal[False], ...]], AlwaysTruthy))
static_assert(is_assignable_to(tuple[int, *tuple[Literal[True], ...]], AlwaysTruthy))

static_assert(is_assignable_to(tuple[*tuple[Any, ...], int], AlwaysTruthy))
static_assert(is_assignable_to(tuple[*tuple[bool, ...], int], AlwaysTruthy))
static_assert(is_assignable_to(tuple[*tuple[Literal[False], ...], int], AlwaysTruthy))
static_assert(is_assignable_to(tuple[*tuple[Literal[True], ...], int], AlwaysTruthy))

static_assert(is_assignable_to(tuple[int, *tuple[Any, ...], int], AlwaysTruthy))
static_assert(is_assignable_to(tuple[int, *tuple[bool, ...], int], AlwaysTruthy))
static_assert(is_assignable_to(tuple[int, *tuple[Literal[False], ...], int], AlwaysTruthy))
static_assert(is_assignable_to(tuple[int, *tuple[Literal[True], ...], int], AlwaysTruthy))
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
