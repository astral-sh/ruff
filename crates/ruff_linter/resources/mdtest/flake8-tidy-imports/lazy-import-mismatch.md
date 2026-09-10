# `lazy-import-mismatch` (`TID254`)

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254"]

[lint.flake8-tidy-imports]
require-lazy = ["typing", "package.deferred"]
ban-lazy = ["this", "package.eager"]
```

## Multiple imports

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

## Multiple imported members

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
