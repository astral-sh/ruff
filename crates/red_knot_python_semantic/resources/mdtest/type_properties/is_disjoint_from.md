# Tests for disjointness

If two types can be disjoint, it means that it is known that no possible runtime object could ever inhabit both types simultaneously.

TODO: Most of our disjointness tests are still Rust tests; they should be moved to this file.

## Instance types versus `type[T]` types

An instance type is disjoint from a `type[T]` type if the instance type is `@final`
and the associated class of the instance type is not a subclass of `T`'s metaclass.

```py
from typing import final
from knot_extensions import is_disjoint_from, static_assert

@final
class Foo: ...

static_assert(is_disjoint_from(Foo, type[int]))
static_assert(is_disjoint_from(type[object], Foo))
static_assert(is_disjoint_from(type[dict], Foo))

# Instance types can be disjoint from `type[]` types
# even if the instance type is a subtype of `type`

@final
class Meta1(type): ...
class UsesMeta1(metaclass=Meta1): ...

static_assert(not is_disjoint_from(Meta1, type[UsesMeta1]))

class Meta2(type): ...
class UsesMeta2(metaclass=Meta2): ...
static_assert(not is_disjoint_from(Meta2, type[UsesMeta2]))
static_assert(is_disjoint_from(Meta1, type[UsesMeta2]))
```
