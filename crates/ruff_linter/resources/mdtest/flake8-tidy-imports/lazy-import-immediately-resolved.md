# `lazy-import-immediately-resolved` (`TID255`)

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254", "TID255"]
flake8-tidy-imports.require-lazy = "all"
```

## Required lazy imports

`TID255` ignores imports that are required to be lazy to avoid a conflict with `TID254`, even if the
import is resolved immediately.

```py
import foo  # snapshot: lazy-import-mismatch

class Bar(foo.Base): ...
```

```snapshot
error[TID254]: Use a `lazy` import instead of an eager import
 --> src/mdtest_snippet.py:1:8
  |
1 | import foo  # snapshot: lazy-import-mismatch
  |        ^^^
help: Convert to a lazy import
  |
  - import foo  # snapshot: lazy-import-mismatch
1 + lazy import foo  # snapshot: lazy-import-mismatch
2 |
  |
note: This is an unsafe fix and may change runtime behavior
```

## Partially required lazy imports

`TID255` still reports immediately resolved names outside `require-lazy`, even when another name in
the same import is required to be lazy.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254", "TID255"]
flake8-tidy-imports.require-lazy = ["foo", "pkg.Base"]
```

```py
lazy import foo as required, bar
lazy from pkg import Base as RequiredBase, OtherBase

required.value
RequiredBase()
bar.value  # error: [lazy-import-immediately-resolved]
OtherBase()  # error: [lazy-import-immediately-resolved]
```

## Module declarations

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID255"]
```

### From imports

A declaration makes members imported from the listed module lazy.

```py
__lazy_modules__ = ["json"]
from json import loads

loads("{}")  # error: [lazy-import-immediately-resolved]
```

### Mixed imports

Only the listed module is lazy, even when the same statement imports an eager module. No fix is
offered for imports governed by `__lazy_modules__`.

```py
__lazy_modules__ = ["json"]
import json, pathlib

json.dumps({})  # snapshot: lazy-import-immediately-resolved
pathlib.Path(".")
```

```snapshot
error[TID255]: Lazy import `json` is resolved immediately
 --> src/mdtest_snippet.py:4:1
  |
4 | json.dumps({})  # snapshot: lazy-import-immediately-resolved
  | ^^^^
```

### Reassigned declarations

Clearing the declaration does not make an earlier import eager.

```py
__lazy_modules__ = ["json"]
import json

__lazy_modules__ = []

json.dumps({})  # error: [lazy-import-immediately-resolved]
```

### Declarations after imports

A declaration does not retroactively make an earlier import lazy.

```py
import json
__lazy_modules__ = ["json"]

json.dumps({})
```

### Older target versions

```toml
target-version = "py39"

[lint]
preview = true
select = ["TID255"]
```

#### Immediate uses

A declaration expresses intended lazy imports even when the target version predates the `lazy`
keyword.

```py
__lazy_modules__ = ["json"]
import json

json.dumps({})  # error: [lazy-import-immediately-resolved]
```

#### Annotations

Ordinary annotations do not immediately resolve lazy imports. On Python versions where
`__lazy_modules__` makes imports lazy (3.15+), annotations are deferred (3.14+), even if the
configured target version is older.

```py
__lazy_modules__ = ["pathlib"]

from pathlib import Path
from typing import Annotated

path: Path  # ok

class C:
    path: Path  # ok

def identity(path: Path) -> Path: ...  # both ok

annotated: Annotated[Path, Path(".")]  # both ok
```

#### Runtime-required annotations

Registering a `singledispatch` implementation evaluates its parameter annotation to determine the
dispatch type, immediately resolving the lazy import.

```py
__lazy_modules__ = ["pathlib"]

from pathlib import Path
from functools import singledispatch

@singledispatch
def process(value): ...

@process.register
def process_path(value: Path): ...  # error: [lazy-import-immediately-resolved]
```

#### Dataclass annotations

The `dataclass` decorator evaluates field annotations, immediately resolving the lazy import.
This is a known false negative.

```py
__lazy_modules__ = ["pathlib"]

from pathlib import Path
from dataclasses import dataclass

@dataclass
class C:
    path: Path  # false negative
```
