# `AlwaysTruthy` and `AlwaysFalsy`

```toml
[environment]
python-version = "3.12"
```

The types `AlwaysTruthy` and `AlwaysFalsy` describe the set of values that are always truthy or
always falsy, respectively. More concretely, a value `at` is of type `AlwaysTruthy` if we can
statically infer that `bool(at)` is always `True`, i.e. that the expression `bool(at)` has type
`Literal[True]`. Conversely, a value `af` is of type `AlwaysFalsy` if we can statically infer that
`bool(af)` is always `False`, i.e. that `bool(af)` has type `Literal[False]`.

## Examples

Here, we give a few examples of values that belong to these types:

```py
from ty_extensions import AlwaysTruthy, AlwaysFalsy
from typing_extensions import Literal

class CustomAlwaysTruthyType:
    def __bool__(self) -> Literal[True]:
        return True

class CustomAlwaysFalsyType:
    def __bool__(self) -> Literal[False]:
        return False

at: AlwaysTruthy
at = True
at = 1
at = 123
at = -1
at = "non empty"
at = b"non empty"
at = CustomAlwaysTruthyType()

af: AlwaysFalsy
af = False
af = None
af = 0
af = ""
af = b""
af = CustomAlwaysFalsyType()
```

## `AlwaysTruthy` and `AlwaysFalsy` are disjoint

It follows directly from the definition that `AlwaysTruthy` and `AlwaysFalsy` are disjoint types:

```py
from ty_extensions import static_assert, is_disjoint_from, AlwaysTruthy, AlwaysFalsy

static_assert(is_disjoint_from(AlwaysTruthy, AlwaysFalsy))
```

## `Truthy` and `Falsy`

It is useful to also define the types `Truthy = ~AlwaysFalsy` and `Falsy = ~AlwaysTruthy`. These
types describe the set of values that *can* be truthy (`bool(t)` can return `True`) or falsy
(`bool(f)` can return `False`), respectively.

Finally, we can also define the type `AmbiguousTruthiness = Truthy & Falsy`, which describes the set
of values that can be truthy *and* falsy. This intersection is not empty. In the following, we give
examples for values that belong to these three types:

```py
from ty_extensions import static_assert, is_equivalent_to, is_disjoint_from, Not, Intersection, AlwaysTruthy, AlwaysFalsy
from typing_extensions import Never
from random import choice

type Truthy = Not[AlwaysFalsy]
type Falsy = Not[AlwaysTruthy]

type AmbiguousTruthiness = Intersection[Truthy, Falsy]

static_assert(is_disjoint_from(AlwaysTruthy, AmbiguousTruthiness))
static_assert(is_disjoint_from(AlwaysFalsy, AmbiguousTruthiness))
static_assert(not is_disjoint_from(Truthy, Falsy))

class CustomAmbiguousTruthinessType:
    def __bool__(self) -> bool:
        return choice((True, False))

def maybe_empty_list() -> list[int]:
    return choice(([], [1, 2, 3]))

reveal_type(bool(maybe_empty_list()))  # revealed: bool
reveal_type(bool(CustomAmbiguousTruthinessType()))  # revealed: bool

t: Truthy
t = True
t = 1
# TODO: This assignment should be okay
t = maybe_empty_list()  # error: [invalid-assignment]
# TODO: This assignment should be okay
t = CustomAmbiguousTruthinessType()  # error: [invalid-assignment]

a: AmbiguousTruthiness
# TODO: This assignment should be okay
a = maybe_empty_list()  # error: [invalid-assignment]
# TODO: This assignment should be okay
a = CustomAmbiguousTruthinessType()  # error: [invalid-assignment]

f: Falsy
f = False
f = None
# TODO: This assignment should be okay
f = maybe_empty_list()  # error: [invalid-assignment]
# TODO: This assignment should be okay
f = CustomAmbiguousTruthinessType()  # error: [invalid-assignment]
```

## Subtypes of `AlwaysTruthy`, `AlwaysFalsy`

```py
from ty_extensions import static_assert, is_subtype_of, is_disjoint_from, AlwaysTruthy, AlwaysFalsy
from typing_extensions import Literal
```

These two types are disjoint, so types (that are not equivalent to Never) can only be a subtype of
either one of them.

```py
static_assert(is_disjoint_from(AlwaysTruthy, AlwaysFalsy))
```

Types that only contain always-truthy values

```py
static_assert(is_subtype_of(Literal[True], AlwaysTruthy))
static_assert(is_subtype_of(Literal[1], AlwaysTruthy))
static_assert(is_subtype_of(Literal[-1], AlwaysTruthy))
static_assert(is_subtype_of(Literal["non empty"], AlwaysTruthy))
static_assert(is_subtype_of(Literal[b"non empty"], AlwaysTruthy))
```

Types that only contain always-falsy values

```py
static_assert(is_subtype_of(None, AlwaysFalsy))
static_assert(is_subtype_of(Literal[False], AlwaysFalsy))
static_assert(is_subtype_of(Literal[0], AlwaysFalsy))
static_assert(is_subtype_of(Literal[""], AlwaysFalsy))
static_assert(is_subtype_of(Literal[b""], AlwaysFalsy))
static_assert(is_subtype_of(Literal[False] | Literal[0], AlwaysFalsy))
```

Ambiguous truthiness types

```py
static_assert(not is_subtype_of(bool, AlwaysTruthy))
static_assert(not is_subtype_of(bool, AlwaysFalsy))

static_assert(not is_subtype_of(list[int], AlwaysTruthy))
static_assert(not is_subtype_of(list[int], AlwaysFalsy))
```

## Open questions

Is `tuple[()]` always falsy? We currently model it this way, but this is
[under discussion](https://github.com/astral-sh/ruff/issues/15528).

```py
from ty_extensions import static_assert, is_subtype_of, AlwaysFalsy

static_assert(is_subtype_of(tuple[()], AlwaysFalsy))
```

## References

See also:

- Our test suite on [narrowing for `if x` and `if not x`](../narrow/truthiness.md).
