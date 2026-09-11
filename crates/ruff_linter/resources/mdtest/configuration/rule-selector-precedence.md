# Rule selector precedence

Categories (e.g. `correctness`, `suspicious`), linter groups (e.g. `RUF`, `UP`), linter prefixes
(e.g. `RUF1`), and rules (e.g. `F401`, `unused-import`) can be combined in the same configuration.
In general, more specific selectors take precedence over broader selectors. Although all categories
aren't strictly "broader" than all linter groups, the general trend still applies. When selectors
have the same specificity, `ignore` takes precedence over `select`. In short, the current precedence
relationship is:

```ignore
ALL < category < linter group < linter prefix < rule
```

## Categories and linter groups can be combined

Select all `F` (`unused-import`) and `restriction` (`assert`) rules.

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

`unused-import` (`F401`) is a `suspicious` rule:

```toml
[lint]
preview = true
select = ["suspicious"]
ignore = ["ALL"]
```

```py
import os  # error: [unused-import]
```

## Linter group selection takes precedence over category ignores

```toml
[lint]
preview = true
select = ["F"]
ignore = ["suspicious"]
```

```py
import os  # error: [unused-import]
```

## Linter group ignores take precedence over category selectors

```toml
[lint]
preview = true
select = ["suspicious"]
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
ignore = ["suspicious"]
```

```py
import os  # error: [unused-import]
```

## Rule codes take precedence over categories

```toml
[lint]
preview = true
select = ["F401"]
ignore = ["suspicious"]
```

```py
import os  # error: [unused-import]
```

## Rule names take precedence over categories

```toml
[lint]
preview = true
select = ["unused-import"]
ignore = ["suspicious"]
```

```py
import os  # error: [unused-import]
```
