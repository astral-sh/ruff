# Assignable-to relation

The `is_assignable_to(S, T)` relation below checks if type `S` is assignable to type `T` (target).
This allows us to check if a type `S` can be used in a context where a type `T` is expected
(function arguments, variable assignments). See the [typing documentation] for a precise definition
of this concept.

## Basic types

### Fully static

Fully static types participate in subtyping. If a type `S` is a subtype of `T`, `S` will also be
assignable to `T`. Two equivalent types are subtypes of each other:

```py
from knot_extensions import static_assert, is_assignable_to

class Parent: ...
class Child1(Parent): ...
class Child2(Parent): ...
class Grandchild(Child1, Child2): ...
class Unrelated: ...

static_assert(is_assignable_to(int, int))
static_assert(is_assignable_to(Parent, Parent))
static_assert(is_assignable_to(Child1, Parent))
static_assert(is_assignable_to(Grandchild, Parent))
static_assert(is_assignable_to(Unrelated, Unrelated))

static_assert(not is_assignable_to(str, int))
static_assert(not is_assignable_to(object, int))
static_assert(not is_assignable_to(Parent, Child1))
static_assert(not is_assignable_to(Unrelated, Parent))
static_assert(not is_assignable_to(Child1, Child2))
```

### Gradual types

Gradual types do not participate in subtyping, but can still be assignable to other types (and
static types can be assignable to gradual types):

```py
from knot_extensions import static_assert, is_assignable_to, Unknown
from typing import Any

static_assert(is_assignable_to(Unknown, Literal[1]))
static_assert(is_assignable_to(Any, Literal[1]))
static_assert(is_assignable_to(Literal[1], Unknown))
static_assert(is_assignable_to(Literal[1], Any))
```

## Literal types

### Boolean literals

`Literal[True]` and `Literal[False]` are both subtypes of (and therefore assignable to) `bool`,
which is in turn a subtype of `int`:

```py
from knot_extensions import static_assert, is_assignable_to
from typing import Literal

static_assert(is_assignable_to(Literal[True], Literal[True]))
static_assert(is_assignable_to(Literal[True], bool))
static_assert(is_assignable_to(Literal[True], int))

static_assert(not is_assignable_to(Literal[True], Literal[False]))
static_assert(not is_assignable_to(bool, Literal[True]))
```

### Integer literals

```py
from knot_extensions import static_assert, is_assignable_to
from typing import Literal

static_assert(is_assignable_to(Literal[1], Literal[1]))
static_assert(is_assignable_to(Literal[1], int))

static_assert(not is_assignable_to(Literal[1], Literal[2]))
static_assert(not is_assignable_to(int, Literal[1]))
static_assert(not is_assignable_to(Literal[1], str))
```

### String literals and `LiteralString`

All string-literal types are subtypes of (and therefore assignable to) `LiteralString`, which is in
turn a subtype of `str`:

```py
from knot_extensions import static_assert, is_assignable_to
from typing_extensions import Literal, LiteralString

static_assert(is_assignable_to(Literal["foo"], Literal["foo"]))
static_assert(is_assignable_to(Literal["foo"], LiteralString))
static_assert(is_assignable_to(Literal["foo"], str))

static_assert(is_assignable_to(LiteralString, str))

static_assert(not is_assignable_to(Literal["foo"], Literal["bar"]))
static_assert(not is_assignable_to(str, Literal["foo"]))
static_assert(not is_assignable_to(str, LiteralString))
```

### Byte literals

```py
from knot_extensions import static_assert, is_assignable_to
from typing_extensions import Literal, LiteralString

static_assert(is_assignable_to(Literal[b"foo"], bytes))
static_assert(is_assignable_to(Literal[b"foo"], Literal[b"foo"]))

static_assert(not is_assignable_to(Literal[b"foo"], str))
static_assert(not is_assignable_to(Literal[b"foo"], LiteralString))
static_assert(not is_assignable_to(Literal[b"foo"], Literal[b"bar"]))
static_assert(not is_assignable_to(Literal[b"foo"], Literal["foo"]))
static_assert(not is_assignable_to(Literal["foo"], Literal[b"foo"]))
```

## `type[…]` and class literals

In the following tests, `TypeOf[str]` is a singleton type with a single inhabitant, the class `str`.
This contrasts with `type[str]`, which represents "all possible subclasses of `str`".

Both `TypeOf[str]` and `type[str]` are subtypes of `type` and `type[object]`, which both represent
"all possible instances of `type`"; therefore both `type[str]` and `TypeOf[str]` are assignable to
`type`. `type[Any]`, on the other hand, represents a type of unknown size or inhabitants, but which
is known to be no larger than the set of possible objects represented by `type`.

```py
from knot_extensions import static_assert, is_assignable_to, Unknown, TypeOf
from typing import Any

static_assert(is_assignable_to(type, type))
static_assert(is_assignable_to(type[object], type[object]))

static_assert(is_assignable_to(type, type[object]))
static_assert(is_assignable_to(type[object], type))

static_assert(is_assignable_to(type[str], type[object]))
static_assert(is_assignable_to(TypeOf[str], type[object]))
static_assert(is_assignable_to(type[str], type))
static_assert(is_assignable_to(TypeOf[str], type))

static_assert(is_assignable_to(type[str], type[str]))
static_assert(is_assignable_to(TypeOf[str], type[str]))

static_assert(not is_assignable_to(TypeOf[int], type[str]))
static_assert(not is_assignable_to(type, type[str]))
static_assert(not is_assignable_to(type[object], type[str]))

static_assert(is_assignable_to(type[Any], type[Any]))
static_assert(is_assignable_to(type[Any], type[object]))
static_assert(is_assignable_to(type[object], type[Any]))
static_assert(is_assignable_to(type, type[Any]))
static_assert(is_assignable_to(type[Any], type[str]))
static_assert(is_assignable_to(type[str], type[Any]))
static_assert(is_assignable_to(TypeOf[str], type[Any]))

static_assert(is_assignable_to(type[Unknown], type[Unknown]))
static_assert(is_assignable_to(type[Unknown], type[object]))
static_assert(is_assignable_to(type[object], type[Unknown]))
static_assert(is_assignable_to(type, type[Unknown]))
static_assert(is_assignable_to(type[Unknown], type[str]))
static_assert(is_assignable_to(type[str], type[Unknown]))
static_assert(is_assignable_to(TypeOf[str], type[Unknown]))

static_assert(is_assignable_to(type[Unknown], type[Any]))
static_assert(is_assignable_to(type[Any], type[Unknown]))

static_assert(not is_assignable_to(object, type[Any]))
static_assert(not is_assignable_to(str, type[Any]))

class Meta(type): ...

static_assert(is_assignable_to(type[Any], Meta))
static_assert(is_assignable_to(type[Unknown], Meta))
static_assert(is_assignable_to(Meta, type[Any]))
static_assert(is_assignable_to(Meta, type[Unknown]))
```

## Tuple types

```py
from knot_extensions import static_assert, is_assignable_to
from typing import Literal, Any

static_assert(is_assignable_to(tuple[()], tuple[()]))
static_assert(is_assignable_to(tuple[int], tuple[int]))
static_assert(is_assignable_to(tuple[int], tuple[Any]))
static_assert(is_assignable_to(tuple[Any], tuple[int]))
static_assert(is_assignable_to(tuple[int, str], tuple[int, str]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[int, int]))
static_assert(is_assignable_to(tuple[Any, Literal[2]], tuple[int, int]))
static_assert(is_assignable_to(tuple[Literal[1], Any], tuple[int, int]))

static_assert(not is_assignable_to(tuple[()], tuple[int]))
static_assert(not is_assignable_to(tuple[int], tuple[str]))
static_assert(not is_assignable_to(tuple[int], tuple[int, str]))
static_assert(not is_assignable_to(tuple[int, str], tuple[int]))
static_assert(not is_assignable_to(tuple[int, int], tuple[Literal[1], int]))
static_assert(not is_assignable_to(tuple[Any, Literal[2]], tuple[int, str]))
```

## Union types

```py
from knot_extensions import static_assert, is_assignable_to, Unknown
from typing import Literal, Any

static_assert(is_assignable_to(int, int | str))
static_assert(is_assignable_to(str, int | str))
static_assert(is_assignable_to(int | str, int | str))
static_assert(is_assignable_to(str | int, int | str))
static_assert(is_assignable_to(Literal[1], int | str))
static_assert(is_assignable_to(Literal[1], Unknown | str))
static_assert(is_assignable_to(Literal[1] | Literal[2], Literal[1] | Literal[2]))
static_assert(is_assignable_to(Literal[1] | Literal[2], int))
static_assert(is_assignable_to(Literal[1] | None, int | None))
static_assert(is_assignable_to(Any, int | str))
static_assert(is_assignable_to(Any | int, int))
static_assert(is_assignable_to(str, int | Any))

static_assert(not is_assignable_to(int | None, int))
static_assert(not is_assignable_to(int | None, str | None))
static_assert(not is_assignable_to(Literal[1] | None, int))
static_assert(not is_assignable_to(Literal[1] | None, str | None))
static_assert(not is_assignable_to(Any | int | str, int))
```

## Intersection types

```py
from knot_extensions import static_assert, is_assignable_to, Intersection, Not
from typing_extensions import Any, Literal

class Parent: ...
class Child1(Parent): ...
class Child2(Parent): ...
class Grandchild(Child1, Child2): ...
class Unrelated: ...

static_assert(is_assignable_to(Intersection[Child1, Child2], Child1))
static_assert(is_assignable_to(Intersection[Child1, Child2], Child2))
static_assert(is_assignable_to(Intersection[Child1, Child2], Parent))
static_assert(is_assignable_to(Intersection[Child1, Parent], Parent))

static_assert(is_assignable_to(Intersection[Parent, Unrelated], Parent))
static_assert(is_assignable_to(Intersection[Child1, Unrelated], Child1))

static_assert(is_assignable_to(Intersection[Child1, Not[Child2]], Child1))
static_assert(is_assignable_to(Intersection[Child1, Not[Child2]], Parent))
static_assert(is_assignable_to(Intersection[Child1, Not[Grandchild]], Parent))

static_assert(is_assignable_to(Intersection[Child1, Child2], Intersection[Child1, Child2]))
static_assert(is_assignable_to(Intersection[Child1, Child2], Intersection[Child2, Child1]))
static_assert(is_assignable_to(Grandchild, Intersection[Child1, Child2]))

static_assert(not is_assignable_to(Parent, Intersection[Parent, Unrelated]))
static_assert(not is_assignable_to(int, Intersection[int, Not[Literal[1]]]))
static_assert(not is_assignable_to(int, Not[int]))
static_assert(not is_assignable_to(int, Not[Literal[1]]))

static_assert(not is_assignable_to(Intersection[Any, Parent], Unrelated))
static_assert(is_assignable_to(Intersection[Unrelated, Any], Intersection[Unrelated, Any]))
static_assert(is_assignable_to(Intersection[Unrelated, Any], Intersection[Unrelated, Not[Any]]))

# TODO: The following assertions should not fail (see https://github.com/astral-sh/ruff/issues/14899)
# error: [static-assert-error]
static_assert(is_assignable_to(Intersection[Any, int], int))
# error: [static-assert-error]
static_assert(is_assignable_to(Intersection[Unrelated, Any], Not[tuple[Unrelated, Any]]))
```

## General properties

See also: our property tests in `property_tests.rs`.

### Everything is assignable to `object`

`object` is Python's top type; the set of all possible objects at runtime:

```py
from knot_extensions import static_assert, is_assignable_to, Unknown
from typing import Literal, Any

static_assert(is_assignable_to(str, object))
static_assert(is_assignable_to(Literal[1], object))
static_assert(is_assignable_to(object, object))
static_assert(is_assignable_to(type, object))
static_assert(is_assignable_to(Any, object))
static_assert(is_assignable_to(Unknown, object))
static_assert(is_assignable_to(type[object], object))
static_assert(is_assignable_to(type[str], object))
static_assert(is_assignable_to(type[Any], object))
```

### Every type is assignable to `Any` / `Unknown`

`Any` and `Unknown` are gradual types. They could materialize to any given type at runtime, and so
any type is assignable to them:

```py
from knot_extensions import static_assert, is_assignable_to, Unknown
from typing import Literal, Any

static_assert(is_assignable_to(str, Any))
static_assert(is_assignable_to(Literal[1], Any))
static_assert(is_assignable_to(object, Any))
static_assert(is_assignable_to(type, Any))
static_assert(is_assignable_to(Any, Any))
static_assert(is_assignable_to(Unknown, Any))
static_assert(is_assignable_to(type[object], Any))
static_assert(is_assignable_to(type[str], Any))
static_assert(is_assignable_to(type[Any], Any))

static_assert(is_assignable_to(str, Unknown))
static_assert(is_assignable_to(Literal[1], Unknown))
static_assert(is_assignable_to(object, Unknown))
static_assert(is_assignable_to(type, Unknown))
static_assert(is_assignable_to(Any, Unknown))
static_assert(is_assignable_to(Unknown, Unknown))
static_assert(is_assignable_to(type[object], Unknown))
static_assert(is_assignable_to(type[str], Unknown))
static_assert(is_assignable_to(type[Any], Unknown))
```

### `Never` is assignable to every type

`Never` is Python's bottom type: the empty set, a type with no inhabitants. It is therefore
assignable to any arbitrary type.

```py
from knot_extensions import static_assert, is_assignable_to, Unknown
from typing_extensions import Never, Any

static_assert(is_assignable_to(Never, str))
static_assert(is_assignable_to(Never, Literal[1]))
static_assert(is_assignable_to(Never, object))
static_assert(is_assignable_to(Never, type))
static_assert(is_assignable_to(Never, Any))
static_assert(is_assignable_to(Never, Unknown))
static_assert(is_assignable_to(Never, type[object]))
static_assert(is_assignable_to(Never, type[str]))
static_assert(is_assignable_to(Never, type[Any]))
```

[typing documentation]: https://typing.readthedocs.io/en/latest/spec/concepts.html#the-assignable-to-or-consistent-subtyping-relation
