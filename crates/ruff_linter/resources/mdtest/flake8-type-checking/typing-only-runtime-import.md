# Typing-only imports (`TC001`, `TC002`, `TC003`)

```toml
target-version = "py315"

[lint]
select = ["TC001", "TC002", "TC003"]

[lint.isort]
known-first-party = ["app"]
```

## Module imports

On Python 3.15, a module-level import used only in annotations can become lazy. This applies to
first-party, third-party, and standard-library imports without enabling preview.

```py
import app as local  # snapshot: typing-only-first-party-import
from vendor import Model  # snapshot: typing-only-third-party-import
import pathlib  # snapshot: typing-only-standard-library-import

def load(value: local.Item, model: Model) -> pathlib.Path: ...
```

```snapshot
error[TC001]: Make application import `app` lazy
 --> src/mdtest_snippet.py:1:15
  |
1 | import app as local  # snapshot: typing-only-first-party-import
  |               ^^^^^
help: Convert to a lazy import (or move into a type-checking block)
  |
  - import app as local  # snapshot: typing-only-first-party-import
1 + lazy import app as local  # snapshot: typing-only-first-party-import
2 | from vendor import Model  # snapshot: typing-only-third-party-import
  |
note: This is an unsafe fix and may change runtime behavior


error[TC002]: Make third-party import `vendor.Model` lazy
 --> src/mdtest_snippet.py:2:20
  |
2 | from vendor import Model  # snapshot: typing-only-third-party-import
  |                    ^^^^^
help: Convert to a lazy import (or move into a type-checking block)
  |
1 | import app as local  # snapshot: typing-only-first-party-import
  - from vendor import Model  # snapshot: typing-only-third-party-import
2 + lazy from vendor import Model  # snapshot: typing-only-third-party-import
3 | import pathlib  # snapshot: typing-only-standard-library-import
  |
note: This is an unsafe fix and may change runtime behavior


error[TC003]: Make standard library import `pathlib` lazy
 --> src/mdtest_snippet.py:3:8
  |
3 | import pathlib  # snapshot: typing-only-standard-library-import
  |        ^^^^^^^
help: Convert to a lazy import (or move into a type-checking block)
  |
2 | from vendor import Model  # snapshot: typing-only-third-party-import
  - import pathlib  # snapshot: typing-only-standard-library-import
3 + lazy import pathlib  # snapshot: typing-only-standard-library-import
4 |
  |
note: This is an unsafe fix and may change runtime behavior
```

## Existing lazy imports

Explicit lazy imports and imports made lazy by `__lazy_modules__` already defer import work and do
not need to move into a type-checking block.

```py
lazy from pathlib import Path  # no diagnostic
__lazy_modules__ = ["vendor"]
from vendor import Model  # no diagnostic

def load(value: Path) -> Model: ...
```

## Multiple imported names

An import with multiple names retains the type-checking-block fix, leaving its runtime-used names
unchanged.

```py
import pathlib, sys  # error: [typing-only-standard-library-import] "type-checking block"

print(sys.version)
def load(value: pathlib.Path): ...
```

## Invalid lazy import locations

Lazy syntax is invalid inside functions and `try` statements, so these imports retain the
type-checking-block fix.

```py
try:
    from vendor import Model  # error: [typing-only-third-party-import] "type-checking block"
except ImportError:
    pass

def load(value: Model):
    from pathlib import Path  # error: [typing-only-standard-library-import] "type-checking block"

    path: Path
```

## Lazy import declarations on older Python versions

A `__lazy_modules__` declaration can express a preference for lazy imports while supporting older
Python versions. Ruff does not edit these declarations, so it falls back to the type-checking-block
fix when the target version does not support the `lazy` keyword.

```toml
target-version = "py314"

[lint]
select = ["TC003"]
```

```py
__lazy_modules__ = ["pathlib"]
from pathlib import Path  # error: [typing-only-standard-library-import] "type-checking block"

def load(value: Path): ...
```

## Type-checking block fixes on older Python versions

```toml
target-version = "py314"
lint.select = ["TC003"]
```

### Late `TYPE_CHECKING` import

No fix is offered when `TYPE_CHECKING` is imported after the annotation that uses the module.

```py
import pathlib  # snapshot: typing-only-standard-library-import

def load(path: pathlib.Path): ...

from typing import TYPE_CHECKING
```

```snapshot
error[TC003]: Move standard library import `pathlib` into a type-checking block
 --> src/mdtest_snippet.py:1:8
  |
1 | import pathlib  # snapshot: typing-only-standard-library-import
  |        ^^^^^^^
help: Move into type-checking block
```

### Inline block

The fix inserts the import into the existing inline block.

```py
from typing import TYPE_CHECKING
import pathlib  # snapshot: typing-only-standard-library-import

if TYPE_CHECKING: import os

def load(path: pathlib.Path): ...
```

```snapshot
error[TC003]: Move standard library import `pathlib` into a type-checking block
 --> src/mdtest_snippet.py:2:8
  |
2 | import pathlib  # snapshot: typing-only-standard-library-import
  |        ^^^^^^^
help: Move into type-checking block
  |
1 | from typing import TYPE_CHECKING
  - import pathlib  # snapshot: typing-only-standard-library-import
2 |
  - if TYPE_CHECKING: import os
3 + if TYPE_CHECKING: import pathlib; import os
4 |
  |
note: This is an unsafe fix and may change runtime behavior
```

### Locally assigned `TYPE_CHECKING`

The fix reuses the existing block without importing `TYPE_CHECKING` from `typing`.

```py
import pathlib  # snapshot: typing-only-standard-library-import

TYPE_CHECKING = False
if TYPE_CHECKING:
    from types import TracebackType

def load(path: pathlib.Path, tb: TracebackType): ...
```

```snapshot
error[TC003]: Move standard library import `pathlib` into a type-checking block
 --> src/mdtest_snippet.py:1:8
  |
1 | import pathlib  # snapshot: typing-only-standard-library-import
  |        ^^^^^^^
help: Move into type-checking block
  |
  - import pathlib  # snapshot: typing-only-standard-library-import
1 |
2 | TYPE_CHECKING = False
3 | if TYPE_CHECKING:
4 +     import pathlib
5 |     from types import TracebackType
  |
note: This is an unsafe fix and may change runtime behavior
```

## Lazy import policies

Imports prohibited by `ban-lazy` retain the type-checking-block fix. An excluded module can still be
imported lazily, including with a `from` import.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TC003", "TID254", "TID255"]

[lint.flake8-tidy-imports]
ban-lazy = { include = "all", exclude = ["pathlib"] }
```

```py
from pathlib import Path  # error: [typing-only-standard-library-import] "lazy"
from decimal import Decimal  # error: [typing-only-standard-library-import] "type-checking block"

def load(value: Path) -> Decimal: ...
```

## Banned lazy imports

With `ban-lazy = "all"`, both cases above receive a type-checking block suggestion.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TC003", "TID254", "TID255"]

[lint.flake8-tidy-imports]
ban-lazy = "all"
```

```py
from pathlib import Path  # error: [typing-only-standard-library-import] "type-checking block"
from decimal import Decimal  # error: [typing-only-standard-library-import] "type-checking block"

def load(value: Path) -> Decimal: ...
```
