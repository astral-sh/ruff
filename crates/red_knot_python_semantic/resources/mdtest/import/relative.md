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
X = 42
```

```py path=package/bar.py
from .foo import X

reveal_type(X)  # revealed: Literal[42]
```

## Dotted

```py path=package/__init__.py
```

```py path=package/foo/bar/baz.py
X = 42
```

```py path=package/bar.py
from .foo.bar.baz import X

reveal_type(X)  # revealed: Literal[42]
```

## Bare to package

```py path=package/__init__.py
X = 42
```

```py path=package/bar.py
from . import X

reveal_type(X)  # revealed: Literal[42]
```

## Non-existent + bare to package

```py path=package/bar.py
from . import X  # error: [unresolved-import]

reveal_type(X)  # revealed: Unknown
```

## Dunder init

```py path=package/__init__.py
from .foo import X

reveal_type(X)  # revealed: Literal[42]
```

```py path=package/foo.py
X = 42
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
X = 42
```

```py path=package/subpackage/subsubpackage/bar.py
from ...foo import X

reveal_type(X)  # revealed: Literal[42]
```

## Unbound symbol

```py path=package/__init__.py
```

```py path=package/foo.py
x
```

```py path=package/bar.py
from .foo import x  # error: [unresolved-import]

reveal_type(x)  # revealed: Unknown
```

## Bare to module

```py path=package/__init__.py
```

```py path=package/foo.py
X = 42
```

```py path=package/bar.py
# TODO: support submodule imports
from . import foo  # error: [unresolved-import]

y = foo.X

# TODO: should be `Literal[42]`
reveal_type(y)  # revealed: Unknown
```

## Non-existent + bare to module

```py path=package/__init__.py
```

```py path=package/bar.py
# TODO: support submodule imports
from . import foo  # error: [unresolved-import]

reveal_type(foo)  # revealed: Unknown
```
