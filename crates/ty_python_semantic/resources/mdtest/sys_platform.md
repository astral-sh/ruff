# `sys.platform`

## Explicit selection of `all` platforms

When `python-platform="all"` is specified, we fall back to the type of `sys.platform` declared in
typeshed:

```toml
[environment]
python-platform = "all"
```

```py
import sys

reveal_type(sys.platform)  # revealed: LiteralString
```

## Explicit selection of a specific platform

```toml
[environment]
python-platform = "linux"
```

```py
import sys

reveal_type(sys.platform)  # revealed: Literal["linux"]
```

## Testing for a specific platform

```toml
[environment]
python-platform = "freebsd8"
```

### Exact comparison

```py
import sys

reveal_type(sys.platform == "freebsd8")  # revealed: Literal[True]
reveal_type(sys.platform == "linux")  # revealed: Literal[False]
```

### Substring comparison

It is [recommended](https://docs.python.org/3/library/sys.html#sys.platform) to use
`sys.platform.startswith(...)` for platform checks:

```py
import sys

reveal_type(sys.platform.startswith("freebsd"))  # revealed: Literal[True]
reveal_type(sys.platform.startswith("linux"))  # revealed: Literal[False]
```
