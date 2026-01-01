# Tuple pair is assignable to their union

Regression test for <https://github.com/astral-sh/ty/issues/2236>.

```toml
[environment]
python-version = "3.11"
```

```py
from types import FunctionType
from ty_extensions import Not, AlwaysTruthy, is_subtype_of, static_assert, is_disjoint_from

class Meta(type): ...
class F(metaclass=Meta): ...

static_assert(not is_subtype_of(tuple[FunctionType, type[F]], Not[tuple[*tuple[AlwaysTruthy, ...], Meta]]))
static_assert(not is_subtype_of(Not[tuple[*tuple[AlwaysTruthy, ...], Meta]], tuple[FunctionType, type[F]]))
static_assert(is_disjoint_from(tuple[FunctionType, type[F]], Not[tuple[*tuple[AlwaysTruthy, ...], Meta]]))
```
