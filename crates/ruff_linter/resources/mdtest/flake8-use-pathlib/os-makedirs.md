# `os-makedirs` (`PTH103`)

## Parent directory permissions

```toml
preview = true
target-version = "py315"
lint.select = ["PTH103"]
```

The conversion preserves `parent_mode` alongside positional `mode` and `exist_ok` arguments,
inserting `True` for `parents` in the position expected by `Path.mkdir`.

```py
import os
from pathlib import Path

os.makedirs("a/b", 0o700, True, parent_mode=0o755)  # snapshot: os-makedirs
```

```snapshot
error[PTH103]: `os.makedirs()` should be replaced by `Path.mkdir(parents=True)`
 --> src/mdtest_snippet.py:4:1
  |
4 | os.makedirs("a/b", 0o700, True, parent_mode=0o755)  # snapshot: os-makedirs
  | ^^^^^^^^^^^
help: Replace with `Path(...).mkdir(parents=True)`
  |
3 |
  - os.makedirs("a/b", 0o700, True, parent_mode=0o755)  # snapshot: os-makedirs
4 + Path("a/b").mkdir(0o700, True, True, parent_mode=0o755)  # snapshot: os-makedirs
5 | os.makedirs("a/b", parent_mode=0o755)  # snapshot: os-makedirs
  |
```

The conversion also preserves `parent_mode` when `mode` and `exist_ok` are omitted.

```py
os.makedirs("a/b", parent_mode=0o755)  # snapshot: os-makedirs
```

```snapshot
error[PTH103]: `os.makedirs()` should be replaced by `Path.mkdir(parents=True)`
 --> src/mdtest_snippet.py:5:1
  |
5 | os.makedirs("a/b", parent_mode=0o755)  # snapshot: os-makedirs
  | ^^^^^^^^^^^
help: Replace with `Path(...).mkdir(parents=True)`
  |
4 | os.makedirs("a/b", 0o700, True, parent_mode=0o755)  # snapshot: os-makedirs
  - os.makedirs("a/b", parent_mode=0o755)  # snapshot: os-makedirs
5 + Path("a/b").mkdir(parents=True, parent_mode=0o755)  # snapshot: os-makedirs
6 | os.makedirs("a/b", 0o700, True, 0o755)  # snapshot: os-makedirs
  |
```

`parent_mode` is keyword-only. Calls with a fourth positional argument receive a diagnostic without a fix.

```py
os.makedirs("a/b", 0o700, True, 0o755)  # snapshot: os-makedirs
```

```snapshot
error[PTH103]: `os.makedirs()` should be replaced by `Path.mkdir(parents=True)`
 --> src/mdtest_snippet.py:6:1
  |
6 | os.makedirs("a/b", 0o700, True, 0o755)  # snapshot: os-makedirs
  | ^^^^^^^^^^^
help: Replace with `Path(...).mkdir(parents=True)`
```
