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

```py
lazy from collections.abc import Collection, Iterable  # no diagnostic

def load(value: Iterable[int]) -> Collection[int]: ...
```

## Multiple imported names

When every imported name is used only for typing, the entire statement can become lazy.

```py
# snapshot: typing-only-standard-library-import
from collections.abc import Collection, Iterable  # error: [typing-only-standard-library-import] "lazy"

def load(values: Iterable[int]) -> Collection[int]: ...
```

```snapshot
error[TC003]: Make standard library import `collections.abc.Collection` lazy
 --> src/mdtest_snippet.py:2:29
  |
2 | from collections.abc import Collection, Iterable  # error: [typing-only-standard-library-import] "lazy"
  |                             ^^^^^^^^^^
help: Convert to a lazy import (or move into a type-checking block)
  |
1 | # snapshot: typing-only-standard-library-import
  - from collections.abc import Collection, Iterable  # error: [typing-only-standard-library-import] "lazy"
2 + lazy from collections.abc import Collection, Iterable  # error: [typing-only-standard-library-import] "lazy"
3 |
  |
note: This is an unsafe fix and may change runtime behavior
```

## First-party imported names

```py
# snapshot: typing-only-first-party-import
from app import Item, Model  # error: [typing-only-first-party-import] "lazy"

def load(value: Item) -> Model: ...
```

```snapshot
error[TC001]: Make application import `app.Item` lazy
 --> src/mdtest_snippet.py:2:17
  |
2 | from app import Item, Model  # error: [typing-only-first-party-import] "lazy"
  |                 ^^^^
help: Convert to a lazy import (or move into a type-checking block)
  |
1 | # snapshot: typing-only-first-party-import
  - from app import Item, Model  # error: [typing-only-first-party-import] "lazy"
2 + lazy from app import Item, Model  # error: [typing-only-first-party-import] "lazy"
3 |
  |
note: This is an unsafe fix and may change runtime behavior
```

## Third-party imported names

```py
# snapshot: typing-only-third-party-import
from vendor import Item, Model  # error: [typing-only-third-party-import] "lazy"

def load(value: Item) -> Model: ...
```

```snapshot
error[TC002]: Make third-party import `vendor.Item` lazy
 --> src/mdtest_snippet.py:2:20
  |
2 | from vendor import Item, Model  # error: [typing-only-third-party-import] "lazy"
  |                    ^^^^
help: Convert to a lazy import (or move into a type-checking block)
  |
1 | # snapshot: typing-only-third-party-import
  - from vendor import Item, Model  # error: [typing-only-third-party-import] "lazy"
2 + lazy from vendor import Item, Model  # error: [typing-only-third-party-import] "lazy"
3 |
  |
note: This is an unsafe fix and may change runtime behavior
```

## Imports from different categories

All categories contribute to deciding whether the entire statement can become lazy.

```py
# snapshot: typing-only-first-party-import
# error: [typing-only-third-party-import] "lazy"
# error: [typing-only-standard-library-import] "lazy"
import app, vendor, pathlib

def load(value: app.Item, model: vendor.Model) -> pathlib.Path: ...
```

```snapshot
error[TC001]: Make application import `app` lazy
 --> src/mdtest_snippet.py:4:8
  |
4 | import app, vendor, pathlib
  |        ^^^
help: Convert to a lazy import (or move into a type-checking block)
  |
3 | # error: [typing-only-standard-library-import] "lazy"
  - import app, vendor, pathlib
4 + lazy import app, vendor, pathlib
5 |
  |
note: This is an unsafe fix and may change runtime behavior
```

## Aliased and parenthesized imports

Adding the keyword preserves aliases and comments without reconstructing the statement.

```py
from collections.abc import (
    Collection as Items,  # snapshot: typing-only-standard-library-import
    Iterable as Values,  # error: [typing-only-standard-library-import] "lazy"
)

def load(values: Values[int]) -> Items[int]: ...
```

```snapshot
error[TC003]: Make standard library import `collections.abc.Collection` lazy
 --> src/mdtest_snippet.py:2:19
  |
2 |     Collection as Items,  # snapshot: typing-only-standard-library-import
  |                   ^^^^^
help: Convert to a lazy import (or move into a type-checking block)
  |
  - from collections.abc import (
1 + lazy from collections.abc import (
2 |     Collection as Items,  # snapshot: typing-only-standard-library-import
  |
note: This is an unsafe fix and may change runtime behavior
```

## Relative imports

```py
# snapshot: typing-only-first-party-import
from . import Item, Model  # error: [typing-only-first-party-import] "lazy"

def load(value: Item) -> Model: ...
```

```snapshot
error[TC001]: Make application import `.Item` lazy
 --> src/mdtest_snippet.py:2:15
  |
2 | from . import Item, Model  # error: [typing-only-first-party-import] "lazy"
  |               ^^^^
help: Convert to a lazy import (or move into a type-checking block)
  |
1 | # snapshot: typing-only-first-party-import
  - from . import Item, Model  # error: [typing-only-first-party-import] "lazy"
2 + lazy from . import Item, Model  # error: [typing-only-first-party-import] "lazy"
3 |
  |
note: This is an unsafe fix and may change runtime behavior
```

## Mixed typing and runtime imports

An import with runtime-used names retains the type-checking-block fix, leaving those names
unchanged.

```py
import pathlib, sys  # error: [typing-only-standard-library-import] "type-checking block"

print(sys.version)
def load(value: pathlib.Path): ...
```

## Mixed imported members in strict mode

Even in strict mode, a runtime-used member prevents converting the entire statement to lazy.

```toml
target-version = "py315"

[lint]
select = ["TC003"]

[lint.flake8-type-checking]
strict = true
```

```py
from collections.abc import Collection, Iterable  # error: [typing-only-standard-library-import] "type-checking block"

print(Iterable)
def load(value: Collection[int]): ...
```

## Unused imported names

An unused name does not qualify for the typing-only fix.

```py
import pathlib, sys  # error: [typing-only-standard-library-import] "type-checking block"

def load(value: pathlib.Path): ...
```

## Shadowed imported names

```py
import pathlib, sys  # error: [typing-only-standard-library-import] "type-checking block"

sys = 1
def load(value: pathlib.Path): ...
```

## Exempt imported names

Exemptions prevent converting an entire statement, even if all names are used in annotations.

```py
import pathlib, typing  # error: [typing-only-standard-library-import] "type-checking block"

def load(value: typing.Iterable[pathlib.Path]): ...
```

## Disabled categories

The fix does not convert names whose typing-only import rule is disabled.

```toml
target-version = "py315"

[lint]
select = ["TC003"]
```

```py
import vendor, pathlib  # error: [typing-only-standard-library-import] "type-checking block"

def load(value: vendor.Model) -> pathlib.Path: ...
```

## Suppressed categories

Suppressing one name's rule keeps the whole-statement lazy fix unavailable. The suppression remains
used.

```toml
target-version = "py315"

[lint]
select = ["TC002", "TC003", "RUF100"]
```

```py
# error: [typing-only-standard-library-import] "type-checking block"
import vendor, pathlib  # noqa: TC002

def load(value: vendor.Model) -> pathlib.Path: ...
```

## Suppressed statements

Suppressions for all imported names remain used without emitting diagnostics.

```toml
target-version = "py315"

[lint]
select = ["TC003", "RUF100"]
```

```py
from collections.abc import Collection, Iterable  # noqa: TC003

def load(value: Iterable[int]) -> Collection[int]: ...
```

## Suppressed imported members

An individual member's suppression also prevents converting the entire statement.

```toml
target-version = "py315"

[lint]
select = ["TC003", "RUF100"]
```

```py
from collections.abc import (
    Collection,  # noqa: TC003
    Iterable,  # error: [typing-only-standard-library-import] "type-checking block"
)

def load(value: Iterable[int]) -> Collection[int]: ...
```

## Invalid lazy import locations

Lazy syntax is invalid inside functions and `try` statements, so these imports retain the
type-checking-block fix.

```py
try:
    # error: [typing-only-third-party-import] "type-checking block"
    from vendor import Model, Item  # error: [typing-only-third-party-import] "type-checking block"
except ImportError:
    pass

def load(value: Model, item: Item):
    # error: [typing-only-standard-library-import] "type-checking block"
    from pathlib import Path, PurePath  # error: [typing-only-standard-library-import] "type-checking block"

    path: Path | PurePath
```

## Class imports

Lazy syntax is also invalid inside class bodies.

```py
class Model:
    from collections.abc import Collection, Iterable  # no diagnostic

    values: Collection[Iterable[int]]
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

## Multiple names on older Python versions

```toml
target-version = "py314"

[lint]
select = ["TC003"]
```

```py
# error: [typing-only-standard-library-import] "type-checking block"
from collections.abc import Collection, Iterable  # error: [typing-only-standard-library-import] "type-checking block"

def load(value: Iterable[int]) -> Collection[int]: ...
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

## Conflicting lazy import policies

If one name must remain eager, the entire statement retains the type-checking-block fix. Applying
the fixes must not alternate between eager and lazy imports.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TC003", "TID254", "TID255"]

[lint.flake8-tidy-imports]
require-lazy = ["pathlib"]
ban-lazy = ["decimal"]
```

```py
# error: [typing-only-standard-library-import] "type-checking block"
# error: [lazy-import-mismatch]
import pathlib, decimal  # error: [typing-only-standard-library-import] "type-checking block"

def load(value: pathlib.Path) -> decimal.Decimal: ...
```

## Banned imported members

A ban on an individual member also prevents converting the entire statement.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TC002", "TID254", "TID255"]

[lint.flake8-tidy-imports]
ban-lazy = ["vendor.Model"]
```

```py
# error: [typing-only-third-party-import] "type-checking block"
from vendor import Item, Model  # error: [typing-only-third-party-import] "type-checking block"

def load(value: Item) -> Model: ...
```

## Excluded modules with multiple members

An exclusion from a blanket ban permits converting all names from that module together.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TC003", "TID254", "TID255"]

[lint.flake8-tidy-imports]
ban-lazy = { include = "all", exclude = ["collections.abc"] }
```

```py
# error: [typing-only-standard-library-import] "lazy"
from collections.abc import Collection, Iterable  # error: [typing-only-standard-library-import] "lazy"

def load(value: Iterable[int]) -> Collection[int]: ...
```

## Banned lazy imports

With `ban-lazy = "all"`, imports retain the type-checking-block fix.

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
