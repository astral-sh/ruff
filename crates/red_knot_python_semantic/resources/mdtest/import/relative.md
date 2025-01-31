# Relative

## Non-existent

```py path=package/__init__.py
```

```py path=package/bar.py
from .foo import X  # error: [unresolved-import]

reveal_type(X)  # revealed: Unknown
```

## Simple

```py path=package/__init__.py
```

```py path=package/foo.py
X: int = 42
```

```py path=package/bar.py
from .foo import X

reveal_type(X)  # revealed: int
```

## Dotted

```py path=package/__init__.py
```

```py path=package/foo/bar/baz.py
X: int = 42
```

```py path=package/bar.py
from .foo.bar.baz import X

reveal_type(X)  # revealed: int
```

## Bare to package

```py path=package/__init__.py
X: int = 42
```

```py path=package/bar.py
from . import X

reveal_type(X)  # revealed: int
```

## Non-existent + bare to package

```py path=package/bar.py
from . import X  # error: [unresolved-import]

reveal_type(X)  # revealed: Unknown
```

## Dunder init

```py path=package/__init__.py
from .foo import X

reveal_type(X)  # revealed: int
```

```py path=package/foo.py
X: int = 42
```

## Non-existent + dunder init

```py path=package/__init__.py
from .foo import X  # error: [unresolved-import]

reveal_type(X)  # revealed: Unknown
```

## Long relative import

```py path=package/__init__.py
```

```py path=package/foo.py
X: int = 42
```

```py path=package/subpackage/subsubpackage/bar.py
from ...foo import X

reveal_type(X)  # revealed: int
```

## Unbound symbol

```py path=package/__init__.py
```

```py path=package/foo.py
x  # error: [unresolved-reference]
```

```py path=package/bar.py
from .foo import x  # error: [unresolved-import]

reveal_type(x)  # revealed: Unknown
```

## Bare to module

```py path=package/__init__.py
```

```py path=package/foo.py
X: int = 42
```

```py path=package/bar.py
from . import foo

reveal_type(foo.X)  # revealed: int
```

## Non-existent + bare to module

This test verifies that we emit an error when we try to import a symbol that is neither a submodule
nor an attribute of `package`.

```py path=package/__init__.py
```

```py path=package/bar.py
from . import foo  # error: [unresolved-import]

reveal_type(foo)  # revealed: Unknown
```

## Import submodule from self

We don't currently consider `from...import` statements when building up the `imported_modules` set
in the semantic index. When accessing an attribute of a module, we only consider it a potential
submodule when that submodule name appears in the `imported_modules` set. That means that submodules
that are imported via `from...import` are not visible to our type inference if you also access that
submodule via the attribute on its parent package.

```py path=package/__init__.py
```

```py path=package/foo.py
X: int = 42
```

```py path=package/bar.py
from . import foo
import package

# error: [unresolved-attribute] "Type `<module 'package'>` has no attribute `foo`"
reveal_type(package.foo.X)  # revealed: Unknown
```
