# `os-path-splitext` (`PTH122`)

```toml
preview = true
lint.select = ["PTH122"]
```

## Extension access

Accessing the second tuple element can be replaced with `Path.suffix`. The fix is unsafe because
path normalization can change the result.

```py
import os

extension = os.path.splitext("report.csv")[1]  # snapshot: os-path-splitext
```

```snapshot
error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:3:13
  |
3 | extension = os.path.splitext("report.csv")[1]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^^
help: Replace with `Path(...).suffix`
  |
1 | import os
2 + import pathlib
3 |
  - extension = os.path.splitext("report.csv")[1]  # snapshot: os-path-splitext
4 + extension = pathlib.Path("report.csv").suffix  # snapshot: os-path-splitext
  |
note: This is an unsafe fix and may change runtime behavior
```

## Negative index and existing imports

The fix reuses an existing alias for `Path`.

```py
from os.path import splitext as split_extension
from pathlib import Path as FilePath

extension = split_extension("report.csv")[-1]  # snapshot: os-path-splitext
```

```snapshot
error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:4:13
  |
4 | extension = split_extension("report.csv")[-1]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^
help: Replace with `Path(...).suffix`
  |
3 |
  - extension = split_extension("report.csv")[-1]  # snapshot: os-path-splitext
4 + extension = FilePath("report.csv").suffix  # snapshot: os-path-splitext
  |
note: This is an unsafe fix and may change runtime behavior
```

## Existing module alias

```py
import os.path as paths
import pathlib as paths_library

extension = paths.splitext("report.csv")[1]  # snapshot: os-path-splitext
```

```snapshot
error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:4:13
  |
4 | extension = paths.splitext("report.csv")[1]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^
help: Replace with `Path(...).suffix`
  |
3 |
  - extension = paths.splitext("report.csv")[1]  # snapshot: os-path-splitext
4 + extension = paths_library.Path("report.csv").suffix  # snapshot: os-path-splitext
  |
note: This is an unsafe fix and may change runtime behavior
```

## Parenthesized expressions

The replacement includes the subscript and preserves any outer operations.

```py
import os

extension = (os.path.splitext(("report.csv")))[1].upper()  # snapshot: os-path-splitext
```

```snapshot
error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:3:14
  |
3 | extension = (os.path.splitext(("report.csv")))[1].upper()  # snapshot: os-path-splitext
  |              ^^^^^^^^^^^^^^^^
help: Replace with `Path(...).suffix`
  |
1 | import os
2 + import pathlib
3 |
  - extension = (os.path.splitext(("report.csv")))[1].upper()  # snapshot: os-path-splitext
4 + extension = pathlib.Path("report.csv").suffix.upper()  # snapshot: os-path-splitext
  |
note: This is an unsafe fix and may change runtime behavior
```

## Function calls

The path expression is evaluated once.

```py
import os

def get_path():
    return "report.csv"

extension = os.path.splitext(get_path())[1]  # snapshot: os-path-splitext
```

```snapshot
error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:6:13
  |
6 | extension = os.path.splitext(get_path())[1]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^^
help: Replace with `Path(...).suffix`
  |
1 | import os
2 + import pathlib
3 |
4 | def get_path():
5 |     return "report.csv"
6 |
  - extension = os.path.splitext(get_path())[1]  # snapshot: os-path-splitext
7 + extension = pathlib.Path(get_path()).suffix  # snapshot: os-path-splitext
  |
note: This is an unsafe fix and may change runtime behavior
```

## Comments

Comments within the replaced expression can be removed by the unsafe fix.

```py
import os

extension = os.path.splitext(  # snapshot: os-path-splitext
    # The report path
    "report.csv"
)[1]
```

```snapshot
error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:3:13
  |
3 | extension = os.path.splitext(  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^^
help: Replace with `Path(...).suffix`
  |
1 | import os
2 + import pathlib
3 |
  - extension = os.path.splitext(  # snapshot: os-path-splitext
  -     # The report path
  -     "report.csv"
  - )[1]
4 + extension = pathlib.Path("report.csv").suffix
  |
note: This is an unsafe fix and may change runtime behavior
```

## Other tuple uses

Keep reporting uses that need a different conversion without offering a fix.

```py
import os

root, extension = os.path.splitext("report.csv")  # snapshot: os-path-splitext
root = os.path.splitext("report.csv")[0]  # snapshot: os-path-splitext
extensions = os.path.splitext("report.csv")[1:]  # snapshot: os-path-splitext
index = 1
extension = os.path.splitext("report.csv")[index]  # snapshot: os-path-splitext
extension = os.path.splitext("report.csv")[2]  # snapshot: os-path-splitext
paths = {}
value = paths[os.path.splitext("report.csv")][1]  # snapshot: os-path-splitext
```

```snapshot
error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:3:19
  |
3 | root, extension = os.path.splitext("report.csv")  # snapshot: os-path-splitext
  |                   ^^^^^^^^^^^^^^^^


error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:4:8
  |
4 | root = os.path.splitext("report.csv")[0]  # snapshot: os-path-splitext
  |        ^^^^^^^^^^^^^^^^


error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:5:14
  |
5 | extensions = os.path.splitext("report.csv")[1:]  # snapshot: os-path-splitext
  |              ^^^^^^^^^^^^^^^^


error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:7:13
  |
7 | extension = os.path.splitext("report.csv")[index]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^^


error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:8:13
  |
8 | extension = os.path.splitext("report.csv")[2]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^^


error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
  --> src/mdtest_snippet.py:10:15
   |
10 | value = paths[os.path.splitext("report.csv")][1]  # snapshot: os-path-splitext
   |               ^^^^^^^^^^^^^^^^
```

## Unsupported arguments

The fix requires one positional argument and skips byte literals because `Path` does not accept
bytes.

```py
import os

args = ("report.csv",)
kwargs = {"p": "report.csv"}
extension = os.path.splitext(*args)[1]  # snapshot: os-path-splitext
extension = os.path.splitext(**kwargs)[1]  # snapshot: os-path-splitext
extension = os.path.splitext(b"report.csv")[1]  # snapshot: os-path-splitext
extension = os.path.splitext(p="report.csv")[1]  # snapshot: os-path-splitext
```

```snapshot
error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:5:13
  |
5 | extension = os.path.splitext(*args)[1]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^^


error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:6:13
  |
6 | extension = os.path.splitext(**kwargs)[1]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^^


error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:7:13
  |
7 | extension = os.path.splitext(b"report.csv")[1]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^^


error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:8:13
  |
8 | extension = os.path.splitext(p="report.csv")[1]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^^
```

## Assignment and deletion targets

Only reading the extension can be fixed.

```py
import os

os.path.splitext("report.csv")[1] = ".txt"  # snapshot: os-path-splitext
del os.path.splitext("report.csv")[-1]  # snapshot: os-path-splitext
```

```snapshot
error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:3:1
  |
3 | os.path.splitext("report.csv")[1] = ".txt"  # snapshot: os-path-splitext
  | ^^^^^^^^^^^^^^^^


error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:4:5
  |
4 | del os.path.splitext("report.csv")[-1]  # snapshot: os-path-splitext
  |     ^^^^^^^^^^^^^^^^
```

## Stable mode

```toml
preview = false
lint.select = ["PTH122"]
```

The diagnostic is available in stable mode while the fix requires preview mode.

```py
import os

extension = os.path.splitext("report.csv")[1]  # snapshot: os-path-splitext
```

```snapshot
error[PTH122]: `os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`
 --> src/mdtest_snippet.py:3:13
  |
3 | extension = os.path.splitext("report.csv")[1]  # snapshot: os-path-splitext
  |             ^^^^^^^^^^^^^^^^
```
