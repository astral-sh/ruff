# `lazy-import-mismatch` (`TID254`)

## Explicit lazy imports

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254"]

[lint.flake8-tidy-imports]
require-lazy = ["typing", "package.deferred"]
ban-lazy = ["this", "package.eager"]
```

### Multiple imports

When names in one statement have conflicting policies, we report the mismatch without a fix.
Changing the entire statement would violate the other name's policy and cause a fix loop.

```py
# snapshot: lazy-import-mismatch
import this, typing

# error: [lazy-import-mismatch]
lazy import this, typing
```

```snapshot
error[TID254]: `typing` should be imported lazily
 --> src/mdtest_snippet.py:2:14
  |
2 | import this, typing
  |              ^^^^^^
help: Convert to a lazy import
```

### Multiple imported members

The same restriction applies to importing multiple members from one module, whether the statement
is eager or lazy.

```py
# snapshot: lazy-import-mismatch
from package import eager, deferred

# error: [lazy-import-mismatch]
lazy from package import eager, deferred
```

```snapshot
error[TID254]: `package.deferred` should be imported lazily
 --> src/mdtest_snippet.py:2:28
  |
2 | from package import eager, deferred
  |                            ^^^^^^^^
help: Convert to a lazy import
```

## Module declarations

```toml
target-version = "py314"

[lint]
preview = true
select = ["TID254"]

[lint.flake8-tidy-imports]
require-lazy = "all"
```

### Literal declarations

[PEP 810](https://peps.python.org/pep-0810/#semantics) allows a module-level `__lazy_modules__`
collection to identify lazy imports. Ruff recognizes literal lists, tuples, and sets, including
annotated assignments.

```py
__lazy_modules__ = ["json"]
import json

__lazy_modules__ = ("pathlib",)
import pathlib

__lazy_modules__: set[str] = {"collections"}
import collections

import math  # error: [lazy-import-mismatch]
```

### Exact module matching

Entries match module names independently of local aliases. A `from` import checks its containing
module, and listing a package does not include its submodules.

```py
__lazy_modules__ = ["json", "xml"]
import json as renamed
from json import dumps, loads
import xml.dom  # error: [lazy-import-mismatch]
```

### Empty declarations on older Python versions

An empty declaration enables lazy-import policies even when the target version predates the `lazy`
keyword.

```py
__lazy_modules__ = []
import json  # error: [lazy-import-mismatch]
```

### Older Python versions without a declaration

Without a declaration, the policy is skipped on target versions that do not support `lazy` syntax.

In the future, the rule could create a `__lazy_modules__` declaration to offer fixes on older targets
without requiring an existing declaration.

```py
import json
```

### Reassigned declarations

Each import uses the declaration present when the checker reaches it. An annotation without a value
does not replace the declaration.

```py
__lazy_modules__ = ["json"]
import json
__lazy_modules__: list[str]
import json as still_lazy

__lazy_modules__ = []
import json as eager  # error: [lazy-import-mismatch]
```

### Unknown declarations

An unsupported declaration value does not establish whether the import violates the policy, so no
diagnostic is emitted.

```py
__lazy_modules__ = configured_modules()
import json
```

### Conditional declarations

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254"]

[lint.flake8-tidy-imports]
require-lazy = ["pathlib", "math", "typing"]
ban-lazy = ["json"]
```

#### Conditional assignment

The declaration may not execute, so `json`'s laziness is unknown and no diagnostic is emitted.

```py
if condition:
    __lazy_modules__ = ["json"]

import json  # ok, may or may not be lazy depending on `condition`
```

#### Conditional reassignments

Modules present in both declarations remain lazy, and modules absent from both remain eager. Added
or removed modules have unknown laziness, so their import policies are not enforced.

```py
__lazy_modules__ = ["json", "pathlib"]
if condition:
    __lazy_modules__ = ["json", "math"]

import json  # error: [lazy-import-mismatch]
import pathlib
import math
import typing  # error: [lazy-import-mismatch]
```

#### Exhaustive branches

Ruff does not prove that branches cover every path, so `json`'s laziness remains unknown even though
both branches list it.

```py
if condition:
    __lazy_modules__ = ["json"]
else:
    __lazy_modules__ = ["json"]

import json  # ok, unknown laziness in our simplified model
```

### Per-alias policies

Each alias is checked independently. Here `json` must become eager and `pathlib` must become lazy.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254"]

[lint.flake8-tidy-imports]
require-lazy = ["pathlib"]
ban-lazy = ["json"]
```

```py
__lazy_modules__ = ["json"]
# error: [lazy-import-mismatch] "`json` should be imported eagerly"
# error: [lazy-import-mismatch] "`pathlib` should be imported lazily"
import json, pathlib
```

### From-import member policies

A declaration makes every member imported from its module lazy, but policies still apply to
individual members.

```toml
target-version = "py314"

[lint]
preview = true
select = ["TID254"]

[lint.flake8-tidy-imports]
require-lazy = ["pkg.First"]
ban-lazy = ["pkg.Second"]
```

```py
__lazy_modules__ = ["pkg"]
from pkg import First
from pkg import Second  # error: [lazy-import-mismatch]
```
