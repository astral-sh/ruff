## Cyclic imports

### Regression tests

#### Issue 261

See: <https://github.com/astral-sh/ty/issues/261>

`main.py`:

```py
from foo import bar

reveal_type(bar)  # revealed: <module 'foo.bar'>
```

`foo/__init__.py`:

```py
from foo import bar

__all__ = ["bar"]
```

`foo/bar/__init__.py`:

```py
# empty
```

#### Issue 113

See: <https://github.com/astral-sh/ty/issues/113>

`main.py`:

```py
from pkg.sub import A

reveal_type(A)  # revealed: Unknown
```

`pkg/outer.py`:

```py
class A: ...
```

`pkg/sub/__init__.py`:

```py
from ..outer import *
from .inner import *
```

`pkg/sub/inner.py`:

```py
from pkg.sub import A
```

### Actual cycle

The following example fails at runtime. Ideally, we would emit a diagnostic here. For now, we only
make sure that this does not lead to a module resolution cycle.

`main.py`:

```py
from module import x

reveal_type(x)  # revealed: Unknown
```

`module.py`:

```py
from module import x
```
