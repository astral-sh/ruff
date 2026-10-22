# `deprecated-mock-import` (`UP026`)

```toml
target-version = "py315"

[lint]
select = ["UP026"]
```

## Only the original module listed

When `unittest` is absent from `__lazy_modules__`, the replacement would make the import eager, so
the diagnostic has no fix.

```py
__lazy_modules__ = ["mock"]
import mock  # snapshot: deprecated-mock-import
```

```snapshot
error[UP026]: `mock` is deprecated, use `unittest.mock`
 --> src/mdtest_snippet.py:2:8
  |
2 | import mock  # snapshot: deprecated-mock-import
  |        ^^^^
help: Import from `unittest.mock` instead
```

## Splitting an import with one replacement module listed

Listing `unittest` preserves laziness for `mock`, but the fix would make `patch` eager because
`unittest.mock` is not listed. The diagnostic has no fix.

```py
__lazy_modules__ = ["mock", "unittest"]
from mock import mock, patch  # snapshot: deprecated-mock-import
```

```snapshot
error[UP026]: `mock` is deprecated, use `unittest.mock`
 --> src/mdtest_snippet.py:2:1
  |
2 | from mock import mock, patch  # snapshot: deprecated-mock-import
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Import from `unittest.mock` instead
```

## Explicit lazy imports

The replacement imports retain `lazy`, including when a `from` import is split.

```py
lazy from mock import mock, patch  # snapshot: deprecated-mock-import
lazy import mock  # snapshot: deprecated-mock-import
```

```snapshot
error[UP026]: `mock` is deprecated, use `unittest.mock`
 --> src/mdtest_snippet.py:1:1
  |
1 | lazy from mock import mock, patch  # snapshot: deprecated-mock-import
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Import from `unittest.mock` instead
  |
  - lazy from mock import mock, patch  # snapshot: deprecated-mock-import
1 + lazy from unittest.mock import patch
2 + lazy from unittest import mock  # snapshot: deprecated-mock-import
3 | lazy import mock  # snapshot: deprecated-mock-import
  |


error[UP026]: `mock` is deprecated, use `unittest.mock`
 --> src/mdtest_snippet.py:2:13
  |
2 | lazy import mock  # snapshot: deprecated-mock-import
  |             ^^^^
help: Import from `unittest.mock` instead
  |
1 | lazy from mock import mock, patch  # snapshot: deprecated-mock-import
  - lazy import mock  # snapshot: deprecated-mock-import
2 + lazy from unittest import mock  # snapshot: deprecated-mock-import
  |
```
