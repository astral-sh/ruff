# Calls to `open()`

## `builtins.open`

We do not fully understand typeshed's overloads for `open()` yet, due to missing support for PEP-613
type aliases. However, we also do not emit false-positive diagnostics on common calls to `open()`:

```py
import pickle

reveal_type(open(""))  # revealed: TextIOWrapper[_WrappedBuffer]
reveal_type(open("", "r"))  # revealed: TextIOWrapper[_WrappedBuffer]
reveal_type(open("", "rb"))  # revealed: BufferedReader[_BufferedReaderStream]

with open("foo.pickle", "rb") as f:
    x = pickle.load(f)  # fine

def _(mode: str):
    reveal_type(open("", mode))  # revealed: IO[Any]
```

## `os.fdopen`

The same is true for `os.fdopen()`:

```py
import pickle
import os

reveal_type(os.fdopen(0))  # revealed: TextIOWrapper[_WrappedBuffer]
reveal_type(os.fdopen(0, "r"))  # revealed: TextIOWrapper[_WrappedBuffer]
reveal_type(os.fdopen(0, "rb"))  # revealed: BufferedReader[_BufferedReaderStream]

with os.fdopen(0, "rb") as f:
    x = pickle.load(f)  # fine
```

## `Path.open`

And similarly for `Path.open()`:

```py
from pathlib import Path
import pickle

reveal_type(Path("").open())  # revealed: TextIOWrapper[_WrappedBuffer]
reveal_type(Path("").open("r"))  # revealed: TextIOWrapper[_WrappedBuffer]
reveal_type(Path("").open("rb"))  # revealed: BufferedReader[_BufferedReaderStream]

with Path("foo.pickle").open("rb") as f:
    x = pickle.load(f)  # fine
```

## `NamedTemporaryFile`

And similarly for `tempfile.NamedTemporaryFile()`:

```py
from tempfile import NamedTemporaryFile
import pickle

reveal_type(NamedTemporaryFile())  # revealed: _TemporaryFileWrapper[bytes]
reveal_type(NamedTemporaryFile("r"))  # revealed: _TemporaryFileWrapper[str]
reveal_type(NamedTemporaryFile("rb"))  # revealed: _TemporaryFileWrapper[bytes]

with NamedTemporaryFile("rb") as f:
    x = pickle.load(f)  # fine
```
