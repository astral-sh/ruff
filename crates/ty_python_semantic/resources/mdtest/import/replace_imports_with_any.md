# Replace imports with Any

When a module cannot be found and matches the pattern, the import is replaced with `Any` and no
diagnostic is emitted.

The syntax uses globe patterns. See `allowed-unresolved-imports` for syntax.

## Unresolvable module is replaced with Any

```toml
[analysis]
replace-imports-with-any = ["foo.**"]
```

```py
import foo
from foo import bar
from foo.sub import baz

reveal_type(foo)  # revealed: Any
reveal_type(bar)  # revealed: Any
reveal_type(baz)  # revealed: Any
```

## Resolvable module is also replaced with Any

Even when the module exists and has type information, its types are replaced with `Any`.

```toml
[analysis]
replace-imports-with-any = ["pkg.**"]
```

`pkg/__init__.py`:

```py
x: int = 1
```

`pkg/sub.py`:

```py
y: str = "hello"
```

`main.py`:

```py
from pkg import x
from pkg.sub import y
import pkg

reveal_type(x)  # revealed: Any
reveal_type(y)  # revealed: Any
reveal_type(pkg)  # revealed: Any
```

## Glob Pattern

```toml
[analysis]
replace-imports-with-any = ["aws*.**"]
```

```py
import aws
import awscli
import awscli.customizations

reveal_type(aws)  # revealed: Any
reveal_type(awscli)  # revealed: Any
reveal_type(awscli.customizations)  # revealed: Any
```

## Negative pattern

```toml
[analysis]
replace-imports-with-any = ["pkg.**", "!pkg.keep"]
```

`pkg/__init__.py`:

```py
```

`pkg/keep.py`:

```py
value: int = 1
```

`main.py`:

```py
from pkg.keep import value
from pkg.skip import other

reveal_type(value)  # revealed: int
reveal_type(other)  # revealed: Any
```

## Relative Imports

The match happens on the module absolute path. If the absolute path that a relative import is
pointing to matches the condition it will be applied.

```toml
[analysis]
replace-imports-with-any = ["**.foo", "bar"]
```

`package/__init__.py`:

```py
```

`package/foo.py`:

```py
val = 1
```

`package/main.py`:

```py
from .foo import val

# .bar would not match "bar" rule because the absolute import is package.bar
from .bar import val2  # error: [unresolved-import]

reveal_type(val)  # revealed: Any
```

## Non-matching modules are unaffected

```toml
[analysis]
replace-imports-with-any = ["skipped.**"]
```

`real_module.py`:

```py
value: int = 42
```

`main.py`:

```py
from real_module import value

reveal_type(value)  # revealed: int
```
