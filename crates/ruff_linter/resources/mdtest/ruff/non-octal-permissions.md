# `non-octal-permissions` (`RUF064`)

## Parent directory permissions

```toml
target-version = "py315"
lint.select = ["RUF064"]
```

The `mode` and `parent_mode` arguments to `os.makedirs` are checked independently.

```py
import os
from pathlib import Path

os.makedirs(
    "a/b",
    mode=700,  # error: [non-octal-permissions] "Non-octal mode"
    parent_mode=755,  # error: [non-octal-permissions] "Non-octal mode"
)
```

`Path.mkdir` also checks `parent_mode`, even when `mode` is omitted, and offers the same octal fix.

```py
Path("a/b").mkdir(parents=True, parent_mode=755)  # snapshot: non-octal-permissions
```

```snapshot
error[RUF064]: Non-octal mode
 --> src/mdtest_snippet.py:9:45
  |
9 | Path("a/b").mkdir(parents=True, parent_mode=755)  # snapshot: non-octal-permissions
  |                                             ^^^
info: Current value of 755 (0o1363) sets permissions: u=-wx, g=rw-, o=-wx
info: Suggested value of 0o755 sets permissions: u=rwx, g=r-x, o=r-x
help: Replace with octal literal
  |
8 | )
  - Path("a/b").mkdir(parents=True, parent_mode=755)  # snapshot: non-octal-permissions
9 + Path("a/b").mkdir(parents=True, parent_mode=0o755)  # snapshot: non-octal-permissions
  |
note: This is an unsafe fix and may change runtime behavior
```
