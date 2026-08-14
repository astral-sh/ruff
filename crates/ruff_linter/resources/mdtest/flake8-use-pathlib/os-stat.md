# `os-stat` (`PTH116`)

## Python 3.9

```toml
preview = true
target-version = "py39"
lint.select = ["PTH116"]
```

`Path.stat` doesn't support the `follow_symlinks` keyword argument before 3.10, so the suggested
fixes have to use either `stat` or `lstat` depending on its value, when it's present.

### `follow_symlinks=True` uses `stat`

```py
import os

os.stat("foo", follow_symlinks=True)  # snapshot: os-stat
```

```snapshot
error[PTH116]: `os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`
 --> src/mdtest_snippet.py:3:1
  |
3 | os.stat("foo", follow_symlinks=True)  # snapshot: os-stat
  | ^^^^^^^
help: Replace with `Path(...).stat()`
  |
1 | import os
2 + import pathlib
3 |
  - os.stat("foo", follow_symlinks=True)  # snapshot: os-stat
4 + pathlib.Path("foo").stat()  # snapshot: os-stat
  |
note: This is an unsafe fix and may change runtime behavior
```

### No `follow_symlinks` also uses `stat`

The default value is `True`, as above:

```py
import os

os.stat("foo")  # snapshot: os-stat
```

```snapshot
error[PTH116]: `os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`
 --> src/mdtest_snippet.py:3:1
  |
3 | os.stat("foo")  # snapshot: os-stat
  | ^^^^^^^
help: Replace with `Path(...).stat()`
  |
1 | import os
2 + import pathlib
3 |
  - os.stat("foo")  # snapshot: os-stat
4 + pathlib.Path("foo").stat()  # snapshot: os-stat
  |
note: This is an unsafe fix and may change runtime behavior
```

### `follow_symlinks=False` uses `lstat`

```py
import os

os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
```

```snapshot
error[PTH116]: `os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`
 --> src/mdtest_snippet.py:3:1
  |
3 | os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
  | ^^^^^^^
help: Replace with `Path(...).lstat()`
  |
1 | import os
2 + import pathlib
3 |
  - os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
4 + pathlib.Path("foo").lstat()  # snapshot: os-stat
  |
note: This is an unsafe fix and may change runtime behavior
```

### Dynamic `follow_symlinks` suppresses the fix

If we can't resolve the value of `follow_symlinks`, we still emit a diagnostic but can't reliably
suggest one of the `stat` methods in a fix.

```py
import os

follow = True

os.stat("foo", follow_symlinks=follow)  # snapshot: os-stat
```

```snapshot
error[PTH116]: `os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`
 --> src/mdtest_snippet.py:5:1
  |
5 | os.stat("foo", follow_symlinks=follow)  # snapshot: os-stat
  | ^^^^^^^
```

## Python 3.10+

```toml
preview = true
target-version = "py310"
lint.select = ["PTH116"]
```

After 3.10, the fixes can always use `stat` and pass along the `follow_symlinks` argument.

```py
import os

os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
```

```snapshot
error[PTH116]: `os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`
 --> src/mdtest_snippet.py:3:1
  |
3 | os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
  | ^^^^^^^
help: Replace with `Path(...).stat()`
  |
1 | import os
2 + import pathlib
3 |
  - os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
4 + pathlib.Path("foo").stat(follow_symlinks=False)  # snapshot: os-stat
5 | follow = True
  |
note: This is an unsafe fix and may change runtime behavior
```

This is also the case for dynamic values:

```py
follow = True

os.stat("foo", follow_symlinks=follow)  # snapshot: os-stat
```

```snapshot
error[PTH116]: `os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`
 --> src/mdtest_snippet.py:6:1
  |
6 | os.stat("foo", follow_symlinks=follow)  # snapshot: os-stat
  | ^^^^^^^
help: Replace with `Path(...).stat()`
  |
1 | import os
2 + import pathlib
3 |
4 | os.stat("foo", follow_symlinks=False)  # snapshot: os-stat
5 | follow = True
6 |
  - os.stat("foo", follow_symlinks=follow)  # snapshot: os-stat
7 + pathlib.Path("foo").stat(follow_symlinks=follow)  # snapshot: os-stat
  |
note: This is an unsafe fix and may change runtime behavior
```
