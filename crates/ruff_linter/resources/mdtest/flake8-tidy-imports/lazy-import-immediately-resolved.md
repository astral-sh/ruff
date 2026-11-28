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

### Eager siblings of lazy submodules

Importing a lazy submodule does not make an already imported parent or sibling lazy.

```py
__lazy_modules__ = {"pkg.a"}
import pkg.b
import pkg.a

class C(pkg.b.B): ...  # ok
class D(pkg.a.A): ...  # error: [lazy-import-immediately-resolved]
parent = pkg  # ok
```

### Eager siblings imported after lazy submodules

An eager sibling import resolves the parent, but leaves the lazy submodule deferred.

```py
__lazy_modules__ = {"pkg.a"}
import pkg.a
import pkg.b

class C(pkg.b.B): ...  # ok
class D(pkg.a.A): ...  # error: [lazy-import-immediately-resolved]
parent = pkg  # ok
```

### Eager parents of lazy submodules

```py
__lazy_modules__ = {"pkg.a"}
import pkg
import pkg.a

parent = pkg  # ok
value = pkg.a.A  # error: [lazy-import-immediately-resolved]
```

### Nested lazy submodules

An eager import can resolve several parent packages without resolving a lazy sibling.

```py
__lazy_modules__ = {"pkg.sub.a"}
import pkg.sub.b
import pkg.sub.a

parent = pkg.sub  # ok
value = pkg.sub.b.B  # ok
lazy_value = pkg.sub.a.A  # error: [lazy-import-immediately-resolved]
```

### Explicit lazy submodules

```py
import pkg.b
lazy import pkg.a

value = pkg.b.B  # ok
lazy_value = pkg.a.A  # error: [lazy-import-immediately-resolved]
```

### Aliased lazy submodules

An alias binds the lazy submodule itself, independently of its parent package.

```py
__lazy_modules__ = {"pkg.a"}
import pkg.b
import pkg.a as a

value = pkg.b.B  # ok
lazy_value = a.A  # error: [lazy-import-immediately-resolved]
```

### Lazy submodules without eager parents

Loading the parent name resolves the parent package when no eager import has loaded it.

```py
__lazy_modules__ = {"pkg.a"}
import pkg.a

parent = pkg  # error: [lazy-import-immediately-resolved]
value = pkg.a.A  # error: [lazy-import-immediately-resolved]
```

### Rebound parent names

Rebinding the parent name disconnects it from earlier lazy submodule imports.

```py
__lazy_modules__ = {"pkg.a"}
import pkg.a

pkg = object()
value = pkg.a  # ok
```

### Replaced lazy attributes

Assigning or deleting a lazy submodule attribute does not load its previous value.

```py
__lazy_modules__ = {"pkg.a", "pkg.b"}
import pkg
import pkg.a
import pkg.b

pkg.a = object()  # ok
del pkg.b  # ok
```

### Eagerly imported lazy submodules

An eager import loads the same submodule even if another import marks it lazy.

```py
lazy import pkg.a
import pkg.a

value = pkg.a.A  # ok
```

### Reassigned declarations with submodules

The declaration at the time of the import determines the submodule's laziness.

```py
__lazy_modules__ = {"pkg.a"}
import pkg.a
import pkg.b

__lazy_modules__ = []
value = pkg.b.B  # ok
lazy_value = pkg.a.A  # error: [lazy-import-immediately-resolved]
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
