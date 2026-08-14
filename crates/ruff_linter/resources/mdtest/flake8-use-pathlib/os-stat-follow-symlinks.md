# `os-stat` (`PTH116`)

## Python 3.9: `follow_symlinks=True` uses `stat`

```toml
preview = true
target-version = "py39"
lint.select = ["PTH116"]
```

```py
import os
os.stat("foo", follow_symlinks=True)  # snapshot: os-stat
```

```snapshot
error[PTH116]: `os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`
 --> src/mdtest_snippet.py:2:1
  |
2 | os.stat("foo", follow_symlinks=True)  # snapshot: os-stat
  | ^^^^^^^
help: Replace with `Path(...).stat()`
  |
1 | import os
  - os.stat("foo", follow_symlinks=True)  # snapshot: os-stat
2 + import pathlib
3 + pathlib.Path("foo").stat()  # snapshot: os-stat
  |
note: This is an unsafe fix and may change runtime behavior
```

## Python 3.9: `follow_symlinks=False` uses `lstat`

```toml
preview = true
target-version = "py39"
lint.select = ["PTH116"]
```

```py
import os
os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
```

```snapshot
error[PTH116]: `os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`
 --> src/mdtest_snippet.py:2:1
  |
2 | os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
  | ^^^^^^^
help: Replace with `Path(...).lstat()`
  |
1 | import os
  - os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
2 + import pathlib
3 + pathlib.Path("foo").lstat()  # snapshot: os-stat
  |
note: This is an unsafe fix and may change runtime behavior
```

## Python >= 3.10: `follow_symlinks=False` uses `stat`

```toml
preview = true
target-version = "py310"
lint.select = ["PTH116"]
```

```py
import os
os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
```

```snapshot
error[PTH116]: `os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`
 --> src/mdtest_snippet.py:2:1
  |
2 | os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
  | ^^^^^^^
help: Replace with `Path(...).stat()`
  |
1 | import os
  - os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
2 + import pathlib
3 + pathlib.Path("foo").stat(follow_symlinks=False)  # snapshot: os-stat
  |
note: This is an unsafe fix and may change runtime behavior
```
