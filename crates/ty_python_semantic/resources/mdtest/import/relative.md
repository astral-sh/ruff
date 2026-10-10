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

## Simple With Stub and Implementation

This is a regression test for an issue with relative imports in implementation files when a stub is
also defined.

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

`package/bar.pyi`:

```pyi
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

# error: [possibly-missing-submodule]
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

## Overlapping search roots, outer root first

When both search roots give a file an importable module name, relative imports use the name from the
deepest root.

```toml
[environment]
extra-paths = ["/src", "/src/nested"]
```

`nested/package/utils.py`:

```py
```

`nested/package/module.py`:

```py
from . import utils

reveal_type(utils)  # revealed: <module 'package.utils'>
```

## Overlapping search roots, inner root first

The deepest root still determines the module name when it appears before the outer root in the
search path list.

```toml
[environment]
extra-paths = ["/src/nested", "/src"]
```

`nested/package/utils.py`:

```py
```

`nested/package/module.py`:

```py
from . import utils

reveal_type(utils)  # revealed: <module 'package.utils'>
```

## Shadowed module name under the deepest root

The file `/src/nested/module.py` is importable as `nested.module`. The name `module`, derived from
the deeper root `/src/nested`, resolves to `/src/module.py` instead because `/src` is searched
first. Relative imports therefore use `nested.module`, derived from `/src`.

```toml
[environment]
extra-paths = ["/src", "/src/nested"]
```

`module.py`:

```py
```

`nested/utils.py`:

```py
```

`nested/module.py`:

```py
from . import utils

reveal_type(utils)  # revealed: <module 'nested.utils'>
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

## Relative imports in a nested editable install

The editable source root is nested inside the project, and the outer directory has the same name as
the installed package. The editable root exposes `pkg` as a top-level package, so `module.py` has
the name `pkg.module`, and its relative import finds `pkg.utils`.

This is a regression test for <https://github.com/astral-sh/ty/issues/4371>.

```toml
[environment]
python = "/.venv"
python-version = "3.13"
```

`/.venv/<path-to-site-packages>/pkg.pth`:

```pth
/src/pkg/src
```

`pkg/src/pkg/__init__.py`:

```py
```

`pkg/src/pkg/utils.py`:

```py
value: int = 42
```

`pkg/src/pkg/module.py`:

```py
from . import utils

reveal_type(utils.value)  # revealed: int
```
