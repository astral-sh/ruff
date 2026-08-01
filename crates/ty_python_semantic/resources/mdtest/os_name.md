# `os.name`

## Explicit selection of `all` platforms

When `python-platform="all"` is specified, we fall back to the type of `os.name` declared in
typeshed:

```toml
[environment]
python-platform = "all"
```

```py
import os

reveal_type(os.name)  # revealed: LiteralString
```

## Explicit selection of a specific platform

### Windows

```toml
[environment]
python-platform = "win32"
```

```py
import os

reveal_type(os.name)  # revealed: Literal["nt"]
```

### Non-Windows

```toml
[environment]
python-platform = "linux"
```

```py
import os

reveal_type(os.name)  # revealed: Literal["posix"]
```

## Testing for a specific platform

### Branch reachability on non-Windows

```toml
[environment]
python-platform = "linux"
```

```py
import os

if os.name == "nt":
    windows = True
else:
    posix = True

# error: [unresolved-reference]
windows

# no error
posix

if os.name == "nt":
    os.startfile("foo.txt")
```

### Branch reachability on Windows

```toml
[environment]
python-platform = "win32"
```

```py
import os

if os.name != "nt":
    os.uname()

if os.name == "nt":
    os.startfile("foo.txt")
```
