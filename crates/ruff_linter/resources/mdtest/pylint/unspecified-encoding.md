# `unspecified-encoding` (`PLW1514`)

```toml
preview = true
lint.select = ["PLW1514"]
```

## Builtin imports

Text files need an explicit encoding even when `open` is imported from `builtins`.

```py
import builtins
from builtins import open as builtin_open

builtins.open("data.txt")  # snapshot: unspecified-encoding
builtin_open("data.txt")  # error: [unspecified-encoding]
```

```snapshot
error[PLW1514]: `builtins.open` in text mode without explicit `encoding` argument
 --> src/mdtest_snippet.py:4:1
  |
4 | builtins.open("data.txt")  # snapshot: unspecified-encoding
  | ^^^^^^^^^^^^^
help: Add explicit `encoding` argument
  |
3 |
  - builtins.open("data.txt")  # snapshot: unspecified-encoding
4 + builtins.open("data.txt", encoding="utf-8")  # snapshot: unspecified-encoding
5 | builtin_open("data.txt")  # error: [unspecified-encoding]
  |
note: This is an unsafe fix and may change runtime behavior
```

## Explicit encodings and binary mode

An explicit encoding or binary mode makes the call valid, whether passed by position or by keyword.

```py
import builtins
from builtins import open as builtin_open

builtins.open("data.txt", encoding="utf-8")
builtin_open("data.txt", "r", -1, "utf-8")
builtins.open("data.bin", "rb")
builtin_open("data.bin", mode="wb")
```
