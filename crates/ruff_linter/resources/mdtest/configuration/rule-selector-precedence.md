# Rule selector precedence

Semantic categories and linter selectors can be combined in the same configuration. More specific
selectors take precedence over broader selectors; when selectors have the same specificity, ignores win.

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

## Semantic categories take precedence over `ALL`

```toml
[lint]
preview = true
select = ["correctness"]
ignore = ["ALL"]
```

```py
import os  # error: [unused-import]
```

## Semantic category ignores take precedence over linter selectors

```toml
[lint]
preview = true
select = ["F"]
ignore = ["correctness"]
```

```py
import os
```

## Linter ignores take precedence over semantic category selectors

```toml
[lint]
preview = true
select = ["correctness"]
ignore = ["F"]
```

```py
import os
```

## Rule prefixes take precedence over semantic categories

```toml
[lint]
preview = true
select = ["F4"]
ignore = ["correctness"]
```

```py
import os  # error: [unused-import]
```

## Rule codes take precedence over semantic categories

```toml
[lint]
preview = true
select = ["F401"]
ignore = ["correctness"]
```

```py
import os  # error: [unused-import]
```

## Rule names take precedence over semantic categories

```toml
[lint]
preview = true
select = ["unused-import"]
ignore = ["correctness"]
```

```py
import os  # error: [unused-import]
```
