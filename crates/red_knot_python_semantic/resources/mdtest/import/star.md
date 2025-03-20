# Wildcard (`*`) imports

## Basic functionality

### A simple `*` import

`a.py`:

```py
X: bool = True
```

`b.py`:

```py
# TODO: should not error
from a import *  # error: [unresolved-import]

# TODO: should not error, should be `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

print(Y)  # error: [unresolved-reference]
```

### Overriding existing definition

`a.py`:

```py
X: bool = True
```

`b.py`:

```py
X = 42
reveal_type(X)  # revealed: Literal[42]

# TODO: should not error
from a import *  # error: [unresolved-import]

# TODO: should reveal `bool`
reveal_type(X)  # revealed: Literal[42]
```

### Overridden by later definition

`a.py`:

```py
X: bool = True
```

`b.py`:

```py
# TODO: should not error
from a import *  # error: [unresolved-import]

# TODO: should not error, should reveal `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

X = 42
reveal_type(X)  # revealed: Literal[42]
```

### Reaching across many modules

`a.py`:

```py
X: bool = True
```

`b.py`:

```py
# TODO: should not error
from a import *  # error: [unresolved-import]
```

`c.py`:

```py
from b import *
```

`d.py`:

```py
from c import *

# TODO: should not error, should reveal `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown
```

### A wildcard import constitutes a re-export

`a.pyi`:

```pyi
X: bool = True
```

`b.pyi`:

```pyi
Y: bool = False
```

`c.pyi`:

```pyi
# TODO: should not error
from a import *  # error: [unresolved-import]
from b import Y
```

`d.py`:

```py
# `X` is accessible because the `*` import in `c` re-exports it from `c`
# TODO: should not error
from c import X  # error: [unresolved-import]

# but `Y` is not because the `from b import Y` import does *not* constitute a re-export
from c import Y  # error: [unresolved-import]
```

### Symbols in statically known branches

```toml
[environment]
python-version = "3.11"
```

`a.py`:

```py
import sys

if sys.version_info >= (3, 11):
    X: bool = True
else:
    Y: bool = False
```

`b.py`:

```py
# TODO should not error
from a import *  # error: [unresolved-import]

# TODO should not error, should reveal `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

## Star imports with `__all__`

If a module `x` contains `__all__`, only symbols included in `x.__all__` are imported by
`from x import *`.

### Simple tuple `__all__`

`a.py`:

```py
__all__ = ("X",)

X: bool = True
Y: bool = False
```

`b.py`:

```py
# TODO should not error
from a import *  # error: [unresolved-import]

# TODO should not error, should reveal `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

### Simple list `__all__`

`a.py`:

```py
__all__ = ["X"]

X: bool = True
Y: bool = False
```

`b.py`:

```py
# TODO should not error
from a import *  # error: [unresolved-import]

# TODO should not error, should reveal `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

### `__all__` with additions later on in the global scope

The [typing spec] lists certain modifications to `__all__` that must be understood by type checkers.

`a.py`:

```py
FOO: bool = True

__all__ = ["FOO"]
```

`b.py`

```py
import a

# TODO should not error
from a import *  # error: [unresolved-import]

__all__ = ["A"]
__all__ += ["B"]
__all__.append("C")
__all__.extend(["D"])
__all__.extend(("E",))
__all__.extend(a.__all__)

A: bool = True
B: bool = True
C: bool = True
D: bool = True
E: bool = True
F: bool = False
```

`c.py`:

```py
# TODO should not error
from b import *  # error: [unresolved-import]

# TODO none of these should error, they should all reveal `bool`
# error: [unresolved-reference]
reveal_type(A)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(B)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(C)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(D)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(E)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(FOO)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(F)  # revealed: Unknown
```

### `__all__` with subtractions later on in the global scope

Whereas there are many ways of adding to `__all__` that type checkers must support, there is only
one way of subtracting from `__all__` that type checkers are required to support:

`a.py`:

```py
__all__ = ["A", "B"]
__all__.remove("A")

A: bool = True
B: bool = True
```

`b.py`:

```py
# TODO should not error
from a import *  # error: [unresolved-import]

# TODO should not error, should reveal `bool`
# error: [unresolved-reference]
reveal_type(A)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(B)  # revealed: Unknown
```

### Invalid `__all__`

If `a.__all__` contains a member that does not refer to a symbol with bindings in the global scope,
a wildcard import from module `a` will fail at runtime.

TODO: Should we:

1. Emit a diagnostic at the invalid definition of `__all__` (which will not fail at runtime)?
1. Emit a diagnostic at the star-import from the module with the invalid `__all__` (which _will_
    fail at runtime)?
1. Emit a diagnostic on both?

`a.py`:

```py
__all__ = ["a", "b"]

a = 42
```

`b.py`:

```py
# TODO even if emiting a diagnostic here is desirable, this is an incorrect error message
# error: [unresolved-import] "Module `a` has no member `*`"
from a import *  # fails with `AttributeError: module 'foo' has no attribute 'b'` at runtime
```

### Dynamic `__all__`

We'll need to decide what to do if `__all__` contains members that are dynamically computed. Mypy
simply ignores any members that are not statically known when determining which symbols are
available (which can lead to false positives).

`a.py`:

```py
def f() -> str:
    return "f"

__all__ = [f()]
```

`b.py`:

```py
# TODO: should not error
from a import *  # error: [unresolved-import]

# Strictly speaking this is a false positive, since there *is* an `f` symbol imported
# by the `*` import at runtime.
#
# error: [unresolved-reference]
reveal_type(f)  # revealed: Unknown
```

### `__all__` combined with statically known branches

```toml
[environment]
python-version = "3.11"
```

`a.py`:

```py
import sys

__all__ = ["X"]
X: bool = True

if sys.version_info >= (3, 11):
    __all__.append("Y")
    Y: bool = True
else:
    __all__.append("Z")
    Z: bool = True
```

`b.py`:

```py
# TODO should not error
from a import *  # error: [unresolved-import]

# TODO neither should error, both should be `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(Z)  # revealed: Unknown
```

## Integration test: `collections.abc`

The `collections.abc` standard-library module provides a good integration test, as all its symbols
are present due to `*` imports.

```py
import typing
import collections.abc

# TODO these should not error, should not reveal `Unknown`
# error: [unresolved-attribute]
reveal_type(collections.abc.Sequence)  # revealed: Unknown
# error: [unresolved-attribute]
reveal_type(collections.abc.Callable)  # revealed: Unknown
```

[typing spec]: https://typing.python.org/en/latest/spec/distributing.html#library-interface-public-and-private-symbols
