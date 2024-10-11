# Follow imports

## Classes

We can follow import to class:

````markdown
```py path=a.py
from b import C as D; E = D
reveal_type(E) # revealed: Literal[C]
```

```py path=b.py
class C: pass
```
````

## Relative

Track that non-existent relative imports resolve to `Unknown`:
````markdown
```py path=package1/__init__.py
```

```py path=package1/bar.py
from .foo import X # error: [unresolved-import]
reveal_type(X)  # revealed: Unknown
```
````

Follow relative imports:

````markdown
```py path=package2/__init__.py
```

```py path=package2/foo.py
X = 42
```

```py path=package2/bar.py
from .foo import X
reveal_type(X)  # revealed: Literal[42]
```
````

We can also follow dotted relative imports:

````markdown
```py path=package3/__init__.py
```

```py path=package3/foo/bar/baz.py
X = 42
```

```py path=package3/bar.py
from .foo.bar.baz import X
reveal_type(X)  # revealed: Literal[42]
```
````

Follow relative import bare to package:

````markdown
```py path=package4/__init__.py
X = 42
```

```py path=package4/bar.py
from . import X
reveal_type(X)  # revealed: Literal[42]
```
````

Follow non-existent relative import bare to package:

```py path=package5/bar.py
from . import X # error: [unresolved-import]
reveal_type(X)  # revealed: Unknown
```

Follow relative import from dunder init:

````markdown
```py path=package6/__init__.py
from .foo import X
reveal_type(X)  # revealed: Literal[42]
```

```py path=package6/foo.py
X = 42
```
````

Follow non-existent relative import from dunder init:

```py path=package7/__init__.py
from .foo import X # error: [unresolved-import]
reveal_type(X)     # revealed: Unknown
```

Follow very relative import:

````markdown
```py path=package8/__init__.py
```

```py path=package8/foo.py
X = 42
```

```py path=package8/subpackage/subsubpackage/bar.py
from ...foo import X
reveal_type(X)  # revealed: Literal[42]
```
````

Imported unbound symbol is `Unknown`:

````markdown
```py path=package9/__init__.py
```

```py path=package9/foo.py
x
```

```py path=package9/bar.py
from .foo import x # error: [unresolved-import]
reveal_type(x)     # revealed: Unknown
```
````
