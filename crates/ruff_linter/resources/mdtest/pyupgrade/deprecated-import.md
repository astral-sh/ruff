# `deprecated-import` (`UP035`)

```toml
target-version = "py315"

[lint]
select = ["UP035"]
```

## Only the original module listed

Rewriting this import would make it eager, so the diagnostic has no fix.

```py
__lazy_modules__ = ["typing"]
from typing import Iterable  # snapshot: deprecated-import
```

```snapshot
error[UP035]: Import from `collections.abc` instead: `Iterable`
 --> src/mdtest_snippet.py:2:1
  |
2 | from typing import Iterable  # snapshot: deprecated-import
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Import from `collections.abc`
```

## Splitting explicit lazy imports

When only some members move to another module, both resulting import statements retain the `lazy`
keyword, even when the replacement module is absent from `__lazy_modules__`.

```py
__lazy_modules__ = ["typing"]
# snapshot: deprecated-import
lazy from typing import Iterable, cast
```

```snapshot
error[UP035]: Import from `collections.abc` instead: `Iterable`
 --> src/mdtest_snippet.py:3:1
  |
3 | lazy from typing import Iterable, cast
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Import from `collections.abc`
  |
2 | # snapshot: deprecated-import
  - lazy from typing import Iterable, cast
3 + lazy from typing import cast
4 + lazy from collections.abc import Iterable
  |
```
