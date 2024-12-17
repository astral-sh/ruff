# `sys.platform`

## Default value

When no target platform is specified, we fall back to the type of `sys.platform` declared in
typeshed:

```toml
[environment]
# No python-platform entry
```

```py
import sys

reveal_type(sys.platform)  # revealed: str
```

## Explicit selection of `all` platforms

```toml
[environment]
python-platform = "all"
```

```py
import sys

reveal_type(sys.platform)  # revealed: str
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
