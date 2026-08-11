# Rule selector precedence

Semantic categories and linter selectors can be combined in the same configuration. More specific
selectors override broader selectors; when selectors have the same specificity, ignores win.

## Semantic categories and linter selectors can be combined

```toml
[lint]
preview = true
select = ["F", "restriction"]
```

```py
import os  # error: [unused-import]
assert True  # error: [assert]
```

## Semantic categories exclude internal test rules

Internal test rules do not belong to user-facing categories and must not be enabled when selecting
one.

```toml
[lint]
preview = true
select = ["pedantic"]
```

`panic.py`:

```py
# Copyright 2026 Astral Software Inc.
"""A documented module."""
x = 1
```

## Semantic categories override `ALL`

```toml
[lint]
preview = true
select = ["correctness"]
ignore = ["ALL"]
```

```py
import os  # error: [unused-import]
```

## Semantic category ignores override linter selectors

```toml
[lint]
preview = true
select = ["F"]
ignore = ["correctness"]
```

```py
import os
```

## Linter ignores override semantic category selectors

```toml
[lint]
preview = true
select = ["correctness"]
ignore = ["F"]
```

```py
import os
```

## Rule prefixes override semantic categories

```toml
[lint]
preview = true
select = ["F4"]
ignore = ["correctness"]
```

```py
import os  # error: [unused-import]
```

## Rule codes override semantic categories

```toml
[lint]
preview = true
select = ["F401"]
ignore = ["correctness"]
```

```py
import os  # error: [unused-import]
```

## Rule names override semantic categories

```toml
[lint]
preview = true
select = ["unused-import"]
ignore = ["correctness"]
```

```py
import os  # error: [unused-import]
```
