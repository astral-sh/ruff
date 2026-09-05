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
os.path.join((p))  # snapshot: os-path-join
```

```snapshot
error[PTH118]: `os.path.join()` should be replaced by `Path` with `/` operator
 --> src/mdtest_snippet.py:4:1
  |
4 | os.path.join((p))  # snapshot: os-path-join
  | ^^^^^^^^^^^^
help: Replace with `Path(...) / ...`
  |
1 | import os
2 + import pathlib
3 |
4 | p = "foo"
  - os.path.join((p))  # snapshot: os-path-join
5 + pathlib.Path(p)  # snapshot: os-path-join
  |
note: This is an unsafe fix and may change runtime behavior
```

## Nested `Path(...)` arguments

Nested `Path(...)` calls are flattened into their leaf arguments

### Nested `Path(...)` argument as the first argument

```py
from pathlib import Path
import os

os.path.join(Path(Path("a"), Path("b")), "c")  # snapshot: os-path-join
```

```snapshot
error[PTH118]: `os.path.join()` should be replaced by `Path` with `/` operator
 --> src/mdtest_snippet.py:4:1
  |
4 | os.path.join(Path(Path("a"), Path("b")), "c")  # snapshot: os-path-join
  | ^^^^^^^^^^^^
help: Replace with `Path(...) / ...`
  |
3 |
  - os.path.join(Path(Path("a"), Path("b")), "c")  # snapshot: os-path-join
4 + Path("a") / "b" / "c"  # snapshot: os-path-join
  |
note: This is an unsafe fix and may change runtime behavior
```

### Nested `Path(...)` argument among the remaining arguments

```py
from pathlib import Path
import os

os.path.join("root", Path(Path("e"), Path("f")))  # snapshot: os-path-join
```

```snapshot
error[PTH118]: `os.path.join()` should be replaced by `Path` with `/` operator
 --> src/mdtest_snippet.py:4:1
  |
4 | os.path.join("root", Path(Path("e"), Path("f")))  # snapshot: os-path-join
  | ^^^^^^^^^^^^
help: Replace with `Path(...) / ...`
  |
3 |
  - os.path.join("root", Path(Path("e"), Path("f")))  # snapshot: os-path-join
4 + Path("root") / "e" / "f"  # snapshot: os-path-join
  |
note: This is an unsafe fix and may change runtime behavior
```

### Chained call is not flattened

`Path("a").resolve()` is a method call on the result of `Path(...)`, not a
bare `Path(...)` call, so it is not flattened

```py
from pathlib import Path
import os

os.path.join("root", "a", "b", Path("c").resolve(), Path("d"), Path(Path("e"), Path("f")))  # snapshot: os-path-join
```

```snapshot
error[PTH118]: `os.path.join()` should be replaced by `Path` with `/` operator
 --> src/mdtest_snippet.py:4:1
  |
4 | os.path.join("root", "a", "b", Path("c").resolve(), Path("d"), Path(Path("e"), Path("f")))  # snapshot: os-path-join
  | ^^^^^^^^^^^^
help: Replace with `Path(...) / ...`
  |
3 |
  - os.path.join("root", "a", "b", Path("c").resolve(), Path("d"), Path(Path("e"), Path("f")))  # snapshot: os-path-join
4 + Path("root") / "a" / "b" / Path("c").resolve() / "d" / "e" / "f"  # snapshot: os-path-join
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
 --> src/mdtest_snippet.py:4:1
  |
4 | os.path.join("root", *parts)  # snapshot: os-path-join
  | ^^^^^^^^^^^^
help: Replace with `Path(...).joinpath(...)`
  |
1 | import os
2 + import pathlib
3 |
4 | parts = ("foo", "bar")
  - os.path.join("root", *parts)  # snapshot: os-path-join
5 + pathlib.Path("root").joinpath(*parts)  # snapshot: os-path-join
  |
note: This is an unsafe fix and may change runtime behavior
```

## `os.sep.join()`

A literal tuple or list passed to `os.sep.join()` can be converted into `Path` components.

### `os.sep.join() is tuple or list`

```py
import os

os.sep.join(["home", "user", "file.txt"]) # snapshot: os-path-join
```

```snapshot
error[PTH118]: `os.sep.join()` should be replaced by `Path` with `/` operator
 --> src/mdtest_snippet.py:3:1
  |
3 | os.sep.join(["home", "user", "file.txt"]) # snapshot: os-path-join
  | ^^^^^^^^^^^
help: Replace with `Path(...) / ...`
  |
1 | import os
2 + import pathlib
3 |
  - os.sep.join(["home", "user", "file.txt"]) # snapshot: os-path-join
4 + pathlib.Path("home") / "user" / "file.txt" # snapshot: os-path-join
  |
note: This is an unsafe fix and may change runtime behavior
```

### `os.sep.join()` with a non-literal argument

When the argument is not a literal tuple or list, no fix is offered.

```py
import os

parts = ["home", "user", "file.txt"]
os.sep.join(parts)  # snapshot: os-path-join
```

```snapshot
error[PTH118]: `os.sep.join()` should be replaced by `Path` with `/` operator
 --> src/mdtest_snippet.py:4:1
  |
4 | os.sep.join(parts)  # snapshot: os-path-join
  | ^^^^^^^^^^^
help: Replace with `Path(...) / ...`
```
