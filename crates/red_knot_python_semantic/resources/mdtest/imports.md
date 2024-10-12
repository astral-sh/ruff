# Follow imports

## Structures

### Class

We can follow import to class:

```py
from b import C as D; E = D
reveal_type(E) # revealed: Literal[C]
```

```py path=b.py
class C: pass
```

### Module member

```py
import b; D = b.C
reveal_type(D) # revealed: Literal[C]
```

```py path=b.py
class C: pass
```

## Relative

### Non-existent

Track that non-existent relative imports resolve to `Unknown`:

```py path=package/__init__.py
```

```py path=package/bar.py
from .foo import X # error: [unresolved-import]
reveal_type(X)  # revealed: Unknown
```

### Simple

We can follow relative imports:

```py path=package/__init__.py
```

```py path=package/foo.py
X = 42
```

```py path=package/bar.py
from .foo import X
reveal_type(X)  # revealed: Literal[42]
```

### Dotted

We can also follow dotted relative imports:

```py path=package/__init__.py
```

```py path=package/foo/bar/baz.py
X = 42
```

```py path=package/bar.py
from .foo.bar.baz import X
reveal_type(X)  # revealed: Literal[42]
```

### Bare to package

We can follow relative import bare to package:

```py path=package/__init__.py
X = 42
```

```py path=package/bar.py
from . import X
reveal_type(X)  # revealed: Literal[42]
```

### Non-existent + bare to package

```py path=package/bar.py
from . import X # error: [unresolved-import]
reveal_type(X)  # revealed: Unknown
```

### Dunder init

```py path=package/__init__.py
from .foo import X
reveal_type(X)  # revealed: Literal[42]
```

```py path=package/foo.py
X = 42
```

### Non-existent + dunder init

```py path=package/__init__.py
from .foo import X # error: [unresolved-import]
reveal_type(X)     # revealed: Unknown
```

### Long relative import

```py path=package/__init__.py
```

```py path=package/foo.py
X = 42
```

```py path=package/subpackage/subsubpackage/bar.py
from ...foo import X
reveal_type(X)  # revealed: Literal[42]
```

### Unbound symbol

We can track that imported unbound symbol is `Unknown`:

```py path=package/__init__.py
```

```py path=package/foo.py
x
```

```py path=package/bar.py
from .foo import x # error: [unresolved-import]
reveal_type(x)     # revealed: Unknown
```

### TODO: Bare to module

Submodule imports possibly not supported right now? Actually, `y` type should be `Literal[42]`.

```py path=package/__init__.py
```

```py path=package/foo.py
X = 42
```

```py path=package/bar.py
from . import foo  # error: [unresolved-import]
y = foo.X
reveal_type(y)     # revealed: Unknown
```

### TODO: Non-existent + bare to module

Submodule imports possibly not supported right now? Actually `foo` import should be resolved correctly.

```py path=package/__init__.py
```

```py path=package/bar.py
from . import foo  # error: [unresolved-import]
reveal_type(foo)   # revealed: Unknown
```
