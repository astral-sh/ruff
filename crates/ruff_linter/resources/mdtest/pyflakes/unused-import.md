# `unused-import` (`F401`)

## Ignore `as` imports that match the default `dummy_variable_rgx`

```toml
[lint]
select = ["F401"]
```

Regression tests for <https://github.com/astral-sh/ruff/issues/25399>

```py
import os as _
import sys as _
```

## Ignore `as` imports matching a custom `dummy_variable_rgx`

```toml
[lint]
select = ["F401"]
dummy-variable-rgx = "^z$"
```

```py
import os as _  # error: [unused-import]
import sys as _  # error: [unused-import]
import zipfile as z
import zipfile as zf  # error: [unused-import]
```
