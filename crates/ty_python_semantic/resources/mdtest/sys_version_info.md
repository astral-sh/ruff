# `sys.version_info`

```toml
[environment]
python-version = "3.9"
```

## The type of `sys.version_info`

The type of `sys.version_info` is `sys._version_info`, at least according to typeshed's stubs (which
we treat as the single source of truth for the standard library). This is quite a complicated type
in typeshed, so there are many things we don't fully understand about the type yet; this is the
source of several TODOs in this test file. Many of these TODOs should be naturally fixed as we
implement more type-system features in the future.

```py
import sys

reveal_type(sys.version_info)  # revealed: _version_info
```

## Literal types from comparisons

Comparing `sys.version_info` with a 2-element tuple of literal integers always produces a `Literal`
type:

```py
import sys

reveal_type(sys.version_info >= (3, 9))  # revealed: Literal[True]
reveal_type((3, 9) <= sys.version_info)  # revealed: Literal[True]

reveal_type(sys.version_info > (3, 9))  # revealed: Literal[True]
reveal_type((3, 9) < sys.version_info)  # revealed: Literal[True]

reveal_type(sys.version_info < (3, 9))  # revealed: Literal[False]
reveal_type((3, 9) > sys.version_info)  # revealed: Literal[False]

reveal_type(sys.version_info <= (3, 9))  # revealed: Literal[False]
reveal_type((3, 9) >= sys.version_info)  # revealed: Literal[False]

reveal_type(sys.version_info == (3, 9))  # revealed: Literal[False]
reveal_type((3, 9) == sys.version_info)  # revealed: Literal[False]

reveal_type(sys.version_info != (3, 9))  # revealed: Literal[True]
reveal_type((3, 9) != sys.version_info)  # revealed: Literal[True]
```

## Non-literal types from comparisons

Comparing `sys.version_info` with tuples of other lengths will sometimes produce `Literal` types,
sometimes not:

```py
import sys

reveal_type(sys.version_info >= (3, 9, 1))  # revealed: bool
reveal_type(sys.version_info >= (3, 9, 1, "final", 0))  # revealed: bool

# TODO: While this won't fail at runtime, the user has probably made a mistake
# if they're comparing a tuple of length >5 with `sys.version_info`
# (`sys.version_info` is a tuple of length 5). It might be worth
# emitting a lint diagnostic of some kind warning them about the probable error?
reveal_type(sys.version_info >= (3, 9, 1, "final", 0, 5))  # revealed: bool

reveal_type(sys.version_info == (3, 8, 1, "finallllll", 0))  # revealed: Literal[False]
```

## Imports and aliases

Comparisons with `sys.version_info` still produce literal types, even if the symbol is aliased to
another name:

```py
from sys import version_info
from sys import version_info as foo

reveal_type(version_info >= (3, 9))  # revealed: Literal[True]
reveal_type(foo >= (3, 9))  # revealed: Literal[True]

bar = version_info
reveal_type(bar >= (3, 9))  # revealed: Literal[True]
```

## Non-stdlib modules named `sys`

Only comparisons with the symbol `version_info` from the `sys` module produce literal types:

`package/__init__.py`:

```py
```

`package/sys.py`:

```py
version_info: tuple[int, int] = (4, 2)
```

`package/script.py`:

```py
from .sys import version_info

reveal_type(version_info >= (3, 9))  # revealed: bool
```

## Accessing fields by name

The fields of `sys.version_info` can be accessed by name:

```py
import sys

reveal_type(sys.version_info.major >= 3)  # revealed: Literal[True]
reveal_type(sys.version_info.minor >= 9)  # revealed: Literal[True]
reveal_type(sys.version_info.minor >= 10)  # revealed: Literal[False]
```

But the `micro`, `releaselevel` and `serial` fields are inferred as `@Todo` until we support
properties on instance types:

```py
reveal_type(sys.version_info.micro)  # revealed: int
reveal_type(sys.version_info.releaselevel)  # revealed: @Todo(Support for `typing.TypeAlias`)
reveal_type(sys.version_info.serial)  # revealed: int
```

## Accessing fields by index/slice

The fields of `sys.version_info` can be accessed by index or by slice:

```py
import sys

reveal_type(sys.version_info[0] < 3)  # revealed: Literal[False]
reveal_type(sys.version_info[1] > 9)  # revealed: Literal[False]

# revealed: tuple[Literal[3], Literal[9], int, Literal["alpha", "beta", "candidate", "final"], int]
reveal_type(sys.version_info[:5])

reveal_type(sys.version_info[:2] >= (3, 9))  # revealed: Literal[True]
reveal_type(sys.version_info[0:2] >= (3, 10))  # revealed: Literal[False]
reveal_type(sys.version_info[:3] >= (3, 10, 1))  # revealed: Literal[False]
reveal_type(sys.version_info[3] == "final")  # revealed: bool
reveal_type(sys.version_info[3] == "finalllllll")  # revealed: Literal[False]
```
