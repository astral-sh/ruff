# Rule selectors

## Rule names in preview

In preview, rule selectors in configuration support both codes and names.

```toml
[lint]
preview = true
select = ["F401", "unused-variable"]
```

```py
import os  # error: [unused-import]


def f():
    value = 1  # error: [unused-variable]
```

## Rule names without preview

Without preview, selectors by name have no effect, while selectors by code continue to apply.

```toml
[lint]
preview = false
select = ["F401", "unused-variable"]
```

```py
import os  # error: [unused-import]


def f():
    value = 1
```
