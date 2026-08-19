# Rule selector precedence

Categories (e.g. `correctness`, `suspicious`), linter groups (e.g. `RUF`, `UP`), linter prefixes
(e.g. `RUF1`), and rules (e.g. `F401`, `unused-import`) can be combined in the same configuration.
More specific selectors take precedence over broader selectors. When selectors have the same
specificity, `ignore` takes precedence over `select`. In short, the current precedence relationship
is:

```ignore
ALL < category = linter group < linter prefix < rule
```

## Categories and linter groups can be combined

```toml
[lint]
preview = true
select = ["F", "restriction"]
```

```py
import os  # error: [unused-import]
assert True  # error: [assert]
```

## Categories take precedence over `ALL`

```toml
[lint]
preview = true
select = ["correctness"]
ignore = ["ALL"]
```

```py
import os  # error: [unused-import]
```

## Category ignores take precedence over linter groups

```toml
[lint]
preview = true
select = ["F"]
ignore = ["correctness"]
```

```py
import os
```

## Linter group ignores take precedence over category selectors

```toml
[lint]
preview = true
select = ["correctness"]
ignore = ["F"]
```

```py
import os
```

## Linter prefixes take precedence over categories

```toml
[lint]
preview = true
select = ["F4"]
ignore = ["correctness"]
```

```py
import os  # error: [unused-import]
```

## Rule codes take precedence over categories

```toml
[lint]
preview = true
select = ["F401"]
ignore = ["correctness"]
```

```py
import os  # error: [unused-import]
```

## Rule names take precedence over categories

```toml
[lint]
preview = true
select = ["unused-import"]
ignore = ["correctness"]
```

```py
import os  # error: [unused-import]
```
