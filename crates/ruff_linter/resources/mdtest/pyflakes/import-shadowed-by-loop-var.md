# `import-shadowed-by-loop-var` (`F402`)

```toml
[lint]
select = ["F402"]
```

## Loop inside a conditional branch

Regression tests for <https://github.com/astral-sh/ruff/issues/30223>. The import and the loop
variable are not in different branches, so the loop variable shadows the import whenever the loop
runs.

```py
from os import path
import json


def f(xs, c):
    if c:
        for path in xs:  # error: [import-shadowed-by-loop-var]
            pass


if c:
    for json in xs:  # error: [import-shadowed-by-loop-var]
        pass
```

## Loop inside a `with` block

```py
import os

with ctx():
    for os in xs:  # error: [import-shadowed-by-loop-var]
        pass
```

## Import inside a conditional branch

```py
if c:
    from os import sep

for sep in xs:  # error: [import-shadowed-by-loop-var]
    pass
```

## Sibling blocks

pyflakes reports a loop in one block that shadows an import from an earlier sibling block, but
Ruff treats sibling blocks like different branches and does not.

```py
if c:
    import glob

if d:
    # TODO: should be an error: [import-shadowed-by-loop-var]
    for glob in xs:
        pass

with ctx():
    import fnmatch

with ctx():
    # TODO: should be an error: [import-shadowed-by-loop-var]
    for fnmatch in xs:
        pass
```

## Different branches

An import and a loop in different branches of the same statement never run together.

```py
if c:
    import re
else:
    for re in xs:  # ok
        pass

try:
    import shlex
except ImportError:
    for shlex in xs:  # ok
        pass

match v:
    case 1:
        import csv
    case _:
        for csv in xs:  # ok
            pass
```
