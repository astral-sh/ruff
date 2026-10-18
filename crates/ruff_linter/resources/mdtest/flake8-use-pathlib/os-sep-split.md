# `os-sep-split` (`PTH206`)

```toml
preview = true
lint.select = ["PTH206"]
```

## Last path component

Reading the last split item can use `Path.name`.

```py
import os

name = "path/to/file.txt".split(os.sep)[-1]  # snapshot: os-sep-split
```

```snapshot
error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:3:27
  |
3 | name = "path/to/file.txt".split(os.sep)[-1]  # snapshot: os-sep-split
  |                           ^^^^^
help: Replace with `Path(...).name`
  |
1 | import os
2 + import pathlib
3 |
  - name = "path/to/file.txt".split(os.sep)[-1]  # snapshot: os-sep-split
4 + name = pathlib.Path("path/to/file.txt").name  # snapshot: os-sep-split
  |
note: This is an unsafe fix and may change runtime behavior
```

## Keyword separator and import aliases

The fix resolves aliases for both the separator and an existing `Path` import.

```py
from os import sep as separator
from pathlib import Path as P

def name(path):
    return path.split(sep=separator)[-1]  # snapshot: os-sep-split
```

```snapshot
error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:5:17
  |
5 |     return path.split(sep=separator)[-1]  # snapshot: os-sep-split
  |                 ^^^^^
help: Replace with `Path(...).name`
  |
4 | def name(path):
  -     return path.split(sep=separator)[-1]  # snapshot: os-sep-split
5 +     return P(path).name  # snapshot: os-sep-split
  |
note: This is an unsafe fix and may change runtime behavior
```

## Module aliases and chained expressions

Only the split and index are replaced. The receiver is evaluated once.

```py
import os as operating_system
import pathlib as paths

def name(get_path):
    return (get_path().split(operating_system.sep)[-1]).upper()  # snapshot: os-sep-split
```

```snapshot
error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:5:24
  |
5 |     return (get_path().split(operating_system.sep)[-1]).upper()  # snapshot: os-sep-split
  |                        ^^^^^
help: Replace with `Path(...).name`
  |
4 | def name(get_path):
  -     return (get_path().split(operating_system.sep)[-1]).upper()  # snapshot: os-sep-split
5 +     return (paths.Path(get_path()).name).upper()  # snapshot: os-sep-split
  |
note: This is an unsafe fix and may change runtime behavior
```

## Parenthesized receivers

Preserve parentheses that are needed by expressions such as `yield`.

```py
import os
from pathlib import Path

def names():
    return (yield "path/to/file.txt").split(os.sep)[-1]  # snapshot: os-sep-split
```

```snapshot
error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:5:39
  |
5 |     return (yield "path/to/file.txt").split(os.sep)[-1]  # snapshot: os-sep-split
  |                                       ^^^^^
help: Replace with `Path(...).name`
  |
4 | def names():
  -     return (yield "path/to/file.txt").split(os.sep)[-1]  # snapshot: os-sep-split
5 +     return Path((yield "path/to/file.txt")).name  # snapshot: os-sep-split
  |
note: This is an unsafe fix and may change runtime behavior
```

## Path normalization is unsafe

Splitting a path ending in a separator returns an empty last item. `Path.name` returns the
preceding component instead. A trailing `.` is also normalized away.

```py
import os
from pathlib import Path

"path/to/".split(os.sep)[-1]  # snapshot: os-sep-split
"path/to/.".split(os.sep)[-1]  # snapshot: os-sep-split
```

```snapshot
error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:4:12
  |
4 | "path/to/".split(os.sep)[-1]  # snapshot: os-sep-split
  |            ^^^^^
help: Replace with `Path(...).name`
  |
3 |
  - "path/to/".split(os.sep)[-1]  # snapshot: os-sep-split
4 + Path("path/to/").name  # snapshot: os-sep-split
5 | "path/to/.".split(os.sep)[-1]  # snapshot: os-sep-split
  |
note: This is an unsafe fix and may change runtime behavior


error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:5:13
  |
5 | "path/to/.".split(os.sep)[-1]  # snapshot: os-sep-split
  |             ^^^^^
help: Replace with `Path(...).name`
  |
4 | "path/to/".split(os.sep)[-1]  # snapshot: os-sep-split
  - "path/to/.".split(os.sep)[-1]  # snapshot: os-sep-split
5 + Path("path/to/.").name  # snapshot: os-sep-split
  |
note: This is an unsafe fix and may change runtime behavior
```

## Comments in the replaced expression

The unsafe fix can remove comments attached to the split or index.

```py
import os
from pathlib import Path

(
    "path/to/file.txt".split(  # snapshot: os-sep-split
        os.sep,  # path separator
    )[-1]
)
```

```snapshot
error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:5:24
  |
5 |     "path/to/file.txt".split(  # snapshot: os-sep-split
  |                        ^^^^^
help: Replace with `Path(...).name`
  |
4 | (
  -     "path/to/file.txt".split(  # snapshot: os-sep-split
  -         os.sep,  # path separator
  -     )[-1]
5 +     Path("path/to/file.txt").name
6 | )
  |
note: This is an unsafe fix and may change runtime behavior
```

## Other uses remain unfixed

Other indices and slices need different replacements. Assignment and deletion targets must
also remain unfixed.

```py
import os

def names(path, index):
    path.split(os.sep)  # snapshot: os-sep-split
    path.split(os.sep)[0]  # snapshot: os-sep-split
    path.split(os.sep)[1]  # snapshot: os-sep-split
    path.split(os.sep)[-2]  # snapshot: os-sep-split
    path.split(os.sep)[-1:]  # snapshot: os-sep-split
    path.split(os.sep)[index]  # snapshot: os-sep-split
    path.split(os.sep)[-1.0]  # snapshot: os-sep-split
    path.split(os.sep)[-True]  # snapshot: os-sep-split
    path.split(os.sep)[-1] = "other"  # snapshot: os-sep-split
    del path.split(os.sep)[-1]  # snapshot: os-sep-split
```

```snapshot
error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:4:10
  |
4 |     path.split(os.sep)  # snapshot: os-sep-split
  |          ^^^^^


error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:5:10
  |
5 |     path.split(os.sep)[0]  # snapshot: os-sep-split
  |          ^^^^^


error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:6:10
  |
6 |     path.split(os.sep)[1]  # snapshot: os-sep-split
  |          ^^^^^


error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:7:10
  |
7 |     path.split(os.sep)[-2]  # snapshot: os-sep-split
  |          ^^^^^


error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:8:10
  |
8 |     path.split(os.sep)[-1:]  # snapshot: os-sep-split
  |          ^^^^^


error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:9:10
  |
9 |     path.split(os.sep)[index]  # snapshot: os-sep-split
  |          ^^^^^


error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
  --> src/mdtest_snippet.py:10:10
   |
10 |     path.split(os.sep)[-1.0]  # snapshot: os-sep-split
   |          ^^^^^


error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
  --> src/mdtest_snippet.py:11:10
   |
11 |     path.split(os.sep)[-True]  # snapshot: os-sep-split
   |          ^^^^^


error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
  --> src/mdtest_snippet.py:12:10
   |
12 |     path.split(os.sep)[-1] = "other"  # snapshot: os-sep-split
   |          ^^^^^


error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
  --> src/mdtest_snippet.py:13:14
   |
13 |     del path.split(os.sep)[-1]  # snapshot: os-sep-split
   |              ^^^^^
```

## Unsupported arguments and byte strings

Calls with `maxsplit` or a different separator are not reported. A bytes receiver still gets
the existing warning but no fix because `Path` rejects bytes.

```py
import os

"path/to/file.txt".split(os.sep, 1)[-1]
"path/to/file.txt".split(sep=os.sep, maxsplit=1)[-1]
"path/to/file.txt".split("/")[-1]
"path/to/file.txt".split(*[os.sep])[-1]
"path/to/file.txt".split(**{"sep": os.sep})[-1]
b"path/to/file.txt".split(os.sep)[-1]  # snapshot: os-sep-split
```

```snapshot
error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:8:21
  |
8 | b"path/to/file.txt".split(os.sep)[-1]  # snapshot: os-sep-split
  |                     ^^^^^
```

## Shadowed separator

A parameter named `sep` is not the imported `os.sep`.

```py
from os import sep

def name(path, sep):
    return path.split(sep)[-1]
```

## Stable mode

```toml
preview = false
lint.select = ["PTH206"]
```

Stable mode keeps the existing warning without offering the fix.

```py
import os

"path/to/file.txt".split(os.sep)[-1]  # snapshot: os-sep-split
```

```snapshot
error[PTH206]: Replace `.split(os.sep)` with `Path.parts`
 --> src/mdtest_snippet.py:3:20
  |
3 | "path/to/file.txt".split(os.sep)[-1]  # snapshot: os-sep-split
  |                    ^^^^^
```
