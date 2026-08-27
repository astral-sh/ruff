# `os-path-join` (`PTH118`)

```toml
preview = true
lint.select = ["PTH118"]
```

## `os.path.join()`

`os.path.join()` can be replaced with `Path` and the `/` operator when its arguments are suitable
path components.

```py
import os

p = "foo"
q = "bar"

os.path.join((p))  # snapshot: os-path-join
```

```snapshot
error[PTH118]: `os.path.join()` should be replaced by `Path` with `/` operator
 --> src/mdtest_snippet.py:6:1
  |
6 | os.path.join((p))  # snapshot: os-path-join
  | ^^^^^^^^^^^^
help: Replace with `Path(...) / ...`
  |
1 | import os
2 + import pathlib
3 |
4 | p = "foo"
5 | q = "bar"
6 |
  - os.path.join((p))  # snapshot: os-path-join
7 + pathlib.Path(p)  # snapshot: os-path-join
  |
note: This is an unsafe fix and may change runtime behavior
```

## Starred arguments

Starred arguments use `Path.joinpath()` instead of `/`.

```py
import os

parts = ("foo", "bar")

os.path.join("root", *parts)  # snapshot: os-path-join
```

```snapshot
error[PTH118]: `os.path.join()` should be replaced by `Path.joinpath()`
 --> src/mdtest_snippet.py:5:1
  |
5 | os.path.join("root", *parts)  # snapshot: os-path-join
  | ^^^^^^^^^^^^
help: Replace with `Path(...).joinpath(...)`
  |
1 | import os
2 + import pathlib
3 |
4 | parts = ("foo", "bar")
5 |
  - os.path.join("root", *parts)  # snapshot: os-path-join
6 + pathlib.Path("root").joinpath(*parts)  # snapshot: os-path-join
  |
note: This is an unsafe fix and may change runtime behavior
```

## `os.sep.join()`

A literal tuple or list passed to `os.sep.join()` can be converted into `Path` components.

```py
import os

os.sep.join("foo")  # snapshot: os-path-join
```

```snapshot
error[PTH118]: `os.sep.join()` should be replaced by `Path` with `/` operator
 --> src/mdtest_snippet.py:3:1
  |
3 | os.sep.join("foo")  # snapshot: os-path-join
  | ^^^^^^^^^^^
help: Replace with `Path(...) / ...`
```
