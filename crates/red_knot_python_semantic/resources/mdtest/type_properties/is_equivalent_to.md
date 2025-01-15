# Equivalence relation

`is_equivalent_to` implements [the equivalence relation] for fully static types.

Two types `A` and `B` are equivalent iff `A` is a subtype of `B` and `B` is a subtype of `A`.

## Basic

```py
from typing import Any
from typing_extensions import Literal
from knot_extensions import Unknown, is_equivalent_to, static_assert

static_assert(is_equivalent_to(Literal[1, 2], Literal[1, 2]))
static_assert(is_equivalent_to(type[object], type))

static_assert(not is_equivalent_to(Any, Any))
static_assert(not is_equivalent_to(Unknown, Unknown))
static_assert(not is_equivalent_to(Any, None))
static_assert(not is_equivalent_to(Literal[1, 2], Literal[1, 0]))
static_assert(not is_equivalent_to(Literal[1, 2], Literal[1, 2, 3]))
```

## Equivalence is commutative

```py
from typing_extensions import Literal
from knot_extensions import is_equivalent_to, static_assert

static_assert(is_equivalent_to(type, type[object]))
static_assert(not is_equivalent_to(Literal[1, 0], Literal[1, 2]))
static_assert(not is_equivalent_to(Literal[1, 2, 3], Literal[1, 2]))
```

## `object & ~T` is equivalent to `~T`

```py
from knot_extensions import Intersection, Not, is_equivalent_to, static_assert

class P: ...

static_assert(is_equivalent_to(Intersection[object, Not[P]], Not[P]))
```

[the equivalence relation]: https://typing.readthedocs.io/en/latest/spec/glossary.html#term-equivalent
