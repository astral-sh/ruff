# Relative

## Non-existent

`package/__init__.py`:

```py
```

`package/bar.py`:

```py
from .foo import X  # error: [unresolved-import]

reveal_type(X)  # revealed: Unknown
```

## Simple

`package/__init__.py`:

```py
```

`package/foo.py`:

```py
X: int = 42
```

`package/bar.py`:

```py
from .foo import X

reveal_type(X)  # revealed: int
```

## Dotted

`package/__init__.py`:

```py
```

`package/foo/bar/baz.py`:

```py
X: int = 42
```

`package/bar.py`:

```py
from .foo.bar.baz import X

reveal_type(X)  # revealed: int
```

## Bare to package

`package/__init__.py`:

```py
X: int = 42
```

`package/bar.py`:

```py
from . import X

reveal_type(X)  # revealed: int
```

## Non-existent + bare to package

`package/bar.py`:

```py
from . import X  # error: [unresolved-import]

reveal_type(X)  # revealed: Unknown
```

## Dunder init

`package/__init__.py`:

```py
from .foo import X

reveal_type(X)  # revealed: int
```

`package/foo.py`:

```py
X: int = 42
```

## Non-existent + dunder init

`package/__init__.py`:

```py
from .foo import X  # error: [unresolved-import]

reveal_type(X)  # revealed: Unknown
```

## Long relative import

`package/__init__.py`:

```py
```

`package/foo.py`:

```py
X: int = 42
```

`package/subpackage/subsubpackage/bar.py`:

```py
from ...foo import X

reveal_type(X)  # revealed: int
```

## Unbound symbol

`package/__init__.py`:

```py
```

`package/foo.py`:

```py
x  # error: [unresolved-reference]
```

`package/bar.py`:

```py
from .foo import x  # error: [unresolved-import]

reveal_type(x)  # revealed: Unknown
```

## Bare to module

`package/__init__.py`:

```py
```

`package/foo.py`:

```py
X: int = 42
```

`package/bar.py`:

```py
from . import foo

reveal_type(foo.X)  # revealed: int
```

## Non-existent + bare to module

This test verifies that we emit an error when we try to import a symbol that is neither a submodule
nor an attribute of `package`.

`package/__init__.py`:

```py
```

`package/bar.py`:

```py
from . import foo  # error: [unresolved-import]

reveal_type(foo)  # revealed: Unknown
```

## Import submodule from self

We don't currently consider `from...import` statements when building up the `imported_modules` set
in the semantic index. When accessing an attribute of a module, we only consider it a potential
submodule when that submodule name appears in the `imported_modules` set. That means that submodules
that are imported via `from...import` are not visible to our type inference if you also access that
submodule via the attribute on its parent package.

`package/__init__.py`:

```py
```

`package/foo.py`:

```py
X: int = 42
```

`package/bar.py`:

```py
from . import foo
import package

# error: [unresolved-attribute] "Type `<module 'package'>` has no attribute `foo`"
reveal_type(package.foo.X)  # revealed: Unknown
```

## Relative imports at the top of a search path

Relative imports at the top of a search path result in a runtime error:
`ImportError: attempted relative import with no known parent package`. That's why ty should disallow
them.

`parser.py`:

```py
X: int = 42
```

`__main__.py`:

```py
from .parser import X  # error: [unresolved-import]
```

## Relative imports in `site-packages`

Relative imports in `site-packages` are correctly resolved even when the `site-packages` search path
is a subdirectory of the first-party search path. Note that mdtest sets the first-party search path
to `/src/`, which is why the virtual environment in this test is a subdirectory of `/src/`, even
though this is not how a typical Python project would be structured:

```toml
[environment]
python = "/src/.venv"
python-version = "3.13"
```

`/src/bar.py`:

```py
from foo import A

reveal_type(A)  # revealed: <class 'A'>
```

`/src/.venv/<path-to-site-packages>/foo/__init__.py`:

```py
from .a import A as A
```

`/src/.venv/<path-to-site-packages>/foo/a.py`:

```py
class A: ...
```
