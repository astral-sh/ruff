# `None`

## `None` as a singleton type

The type `None` (or `NoneType`, see below) is a singleton type that has only one inhabitant: the
object `None`.

```py
from ty_extensions import static_assert, is_singleton, is_equivalent_to

n: None = None

static_assert(is_singleton(None))
```

Just like for other singleton types, the only subtypes of `None` are `None` itself and `Never`:

```py
from ty_extensions import static_assert, is_subtype_of
from typing_extensions import Never

static_assert(is_subtype_of(None, None))
static_assert(is_subtype_of(Never, None))
```

## Relationship to `Optional[T]`

The type `Optional[T]` is an alias for `T | None` (or `Union[T, None]`):

```py
from ty_extensions import static_assert, is_equivalent_to
from typing import Optional, Union

class T: ...

static_assert(is_equivalent_to(Optional[T], T | None))
static_assert(is_equivalent_to(Optional[T], Union[T, None]))
```

## Type narrowing using `is`

Just like for other singleton types, we support type narrowing using `is` or `is not` checks:

```py
from typing_extensions import assert_type

class T: ...

def f(x: T | None):
    if x is None:
        assert_type(x, None)
    else:
        assert_type(x, T)

    assert_type(x, T | None)

    if x is not None:
        assert_type(x, T)
    else:
        assert_type(x, None)
```

## `NoneType`

`None` is special in that the name of the instance at runtime can be used as a type as well: The
object `None` is an instance of type `None`. When a distinction between the two is needed, the
spelling `NoneType` can be used, which is available since Python 3.10. `NoneType` is equivalent to
`None`:

```toml
[environment]
python-version = "3.10"
```

```py
from ty_extensions import static_assert, is_equivalent_to
from types import NoneType

static_assert(is_equivalent_to(NoneType, None))
```
