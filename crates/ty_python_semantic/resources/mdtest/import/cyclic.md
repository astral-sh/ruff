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

# TODO: This should be `<class 'A'>`
reveal_type(A)  # revealed: Never
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
# error: [unresolved-import]
from module import x
```

### Normal self-referential import

Some modules like `sys` in typeshed import themselves. Here, we make sure that this does not lead to
cycles or unresolved imports.

`module/__init__.py`:

```py
import module  # self-referential import

from module.sub import x
```

`module/sub.py`:

```py
x: int = 1
```

`main.py`:

```py
from module import x

reveal_type(x)  # revealed: int
```
