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

## Rule names in per-file ignores

Per-file ignores also support rule names only in preview.

### Preview enabled

```toml
[lint]
preview = true
select = ["F401"]
per-file-ignores = { "mdtest_snippet.py" = ["unused-import"] }
```

```py
import os
```

### Preview disabled

```toml
[lint]
preview = false
select = ["F401"]
per-file-ignores = { "mdtest_snippet.py" = ["unused-import"] }
```

```py
import os  # error: [unused-import]
```

## Rule names in fixability selectors

Rule names in `fixable` and `unfixable` only affect fix availability in preview.

### `fixable` with preview enabled

```toml
[lint]
preview = true
select = ["F401"]
fixable = ["unused-import"]
```

```py
import os  # snapshot: unused-import
```

```snapshot
error[F401]: `os` imported but unused
 --> src/mdtest_snippet.py:1:8
  |
1 | import os  # snapshot: unused-import
  |        ^^
  |
help: Remove unused import: `os`
  |
  - import os  # snapshot: unused-import
  |
```

### `fixable` with preview disabled

```toml
[lint]
preview = false
select = ["F401"]
fixable = ["unused-import"]
```

```py
import os  # snapshot: unused-import
```

```snapshot
error[F401]: `os` imported but unused
 --> src/mdtest_snippet.py:1:8
  |
1 | import os  # snapshot: unused-import
  |        ^^
  |
help: Remove unused import: `os`
```

### `unfixable` with preview enabled

```toml
[lint]
preview = true
select = ["F401"]
unfixable = ["unused-import"]
```

```py
import os  # snapshot: unused-import
```

```snapshot
error[F401]: `os` imported but unused
 --> src/mdtest_snippet.py:1:8
  |
1 | import os  # snapshot: unused-import
  |        ^^
  |
help: Remove unused import: `os`
```

### `unfixable` with preview disabled

```toml
[lint]
preview = false
select = ["F401"]
unfixable = ["unused-import"]
```

```py
import os  # snapshot: unused-import
```

```snapshot
error[F401]: `os` imported but unused
 --> src/mdtest_snippet.py:1:8
  |
1 | import os  # snapshot: unused-import
  |        ^^
  |
help: Remove unused import: `os`
  |
  - import os  # snapshot: unused-import
  |
```
