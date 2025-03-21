# Wildcard (`*`) imports

See the [Python language reference for import statements].

## Basic functionality

### A simple `*` import

`a.py`:

```py
X: bool = True
```

`b.py`:

```py
from a import *

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

from a import *

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
from a import *

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
from a import *
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
from a import *
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

### Global-scope symbols defined using walrus expressions

`a.py`:

```py
X = (Y := 3) + 4
```

`b.py`:

```py
from a import *

# TODO should not error, should reveal `Literal[7] | Unknown`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown
# TODO should not error, should reveal `Literal[3] | Unknown`
# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

### Global-scope names starting with underscores

Global-scope names starting with underscores are not imported from a `*` import (unless the module
has `__all__` and they are included in `__all__`):

`a.py`:

```py
_private: bool = False
__protected: bool = False
__dunder__: bool = False
___thunder___: bool = False

Y: bool = True
```

`b.py`:

```py
from a import *

# These errors are correct:
#
# error: [unresolved-reference]
reveal_type(_private)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(__protected)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(__dunder__)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(___thunder___)  # revealed: Unknown

# TODO: this error is incorrect (should reveal `bool`):
#
# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

### All public symbols are considered re-exported from `.py` files

For `.py` files, we should consider all public symbols in the global namespace exported by that
module when considering which symbols are made available by a `*` import. Here, `b.py` does not use
the explicit `from a import X as X` syntax to explicitly mark it as publicly re-exported, and `X` is
not included in `__all__`; whether it should be considered a "public name" in module `b` is
ambiguous. We could consider an opt-in rule to warn the user when they use `X` in `c.py` that it was
not included in `__all__` and was not marked as an explicit re-export.

`a.py`:

```py
X: bool = True
```

`b.py`:

```py
from a import X
```

`c.py`:

```py
from b import *

# TODO: this is a false positive, but we could consider a different opt-in diagnostic
# (see prose commentary above)
#
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown
```

### Only explicit re-exports are considered re-exported from `.pyi` files

For `.pyi` files, we should consider all imports private to the stub unless they are included in
`__all__` or use the explict `from foo import X as X` syntax.

`a.pyi`:

```pyi
X: bool = True
```

`b.pyi`:

```pyi
from a import X
```

`c.py`:

```py
from b import *

# This error is correct, as `X` is not considered re-exported from module `b`:
#
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown
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
from a import *

# TODO should not error, should reveal `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

### Relative `*` imports

Relative `*` imports are also supported by Python:

`a/__init__.py`:

```py
```

`a/foo.py`:

```py
X: bool = True
```

`a/bar.py`:

```py
from .foo import *

# TODO should not error, should reveal `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown
```

## Star imports with `__all__`

If a module `x` contains `__all__`, all symbols included in `x.__all__` are imported by
`from x import *` (but no other symbols are).

### Simple tuple `__all__`

`a.py`:

```py
__all__ = ("X", "_private", "__protected", "__dunder__", "___thunder___")

X: bool = True
_private: bool = True
__protected: bool = True
__dunder__: bool = True
___thunder___: bool = True

Y: bool = False
```

`b.py`:

```py
from a import *

# TODO none of these should error, should all reveal `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(_private)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(__protected)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(__dunder__)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(___thunder___)  # revealed: Unknown

# but this diagnostic is accurate!
#
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
from a import *

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

`b.py`:

```py
import a
from a import *

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
from b import *

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
from a import *

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
# TODO we should consider emitting a diagnostic here (see prose description above)
from a import *  # fails with `AttributeError: module 'foo' has no attribute 'b'` at runtime
```

### Dynamic `__all__`

If `__all__` contains members that are dynamically computed, we should check that all members of
`__all__` are assignable to `str`. For the purposes of evaluating `*` imports, however, we should
treat the module as though it has no `__all__` at all: all global-scope members of the module should
be considered imported by the import statement. We should probably also emit a warning telling the
user that we cannot statically determine the elements of `__all__`.

`a.py`:

```py
def f() -> str:
    return "f"

def g() -> int:
    return 42

# TODO we should emit a warning here for the dynamically constructed `__all__` member.
__all__ = [f()]
```

`b.py`:

```py
from a import *

# TODO: we should avoid both errors here.
#
# At runtime, `f` is imported but `g` is not; to avoid false positives, however,
# we should treat `a` as though it does not have `__all__` at all,
# which would imply that both symbols would be present.
#
# error: [unresolved-reference]
reveal_type(f)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(g)  # revealed: Unknown
```

### `__all__` conditionally defined in a statically known branch

```toml
[environment]
python-version = "3.11"
```

`a.py`:

```py
import sys

X: bool = True

if sys.version_info >= (3, 11):
    __all__ = ["X", "Y"]
    Y: bool = True
else:
    __all__ = ("Z",)
    Z: bool = True
```

`b.py`:

```py
from a import *

# TODO neither should error, both should be `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(Z)  # revealed: Unknown
```

### `__all__` conditionally mutated in a statically known branch

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
from a import *

# TODO neither should error, both should be `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(Z)  # revealed: Unknown
```

### Empty `__all__`

An empty `__all__` is valid, but a `*` import from a module with an empty `__all__` results in 0
bindings being added from the import:

`a.py`:

```py
X: bool = True

__all__ = ()
```

`b.py`:

```py
Y: bool = True

__all__ = []
```

`c.py`:

```py
from a import *
from b import *

# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

### `__all__` in a stub file

If a name is included in `__all__` in a stub file, it is considered re-exported even if it was only
defined using an import without the explicit `from foo import X as X` syntax:

`a.py`:

```py
X: bool = True
Y: bool = True
```

`b.py`:

```py
from a import X, Y

__all__ = ["X"]
```

`c.py`:

```py
from b import *

# TODO: should not error, should reveal `bool`
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

# this error is correct:
# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
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

## Invalid `*` imports

### Unresolved module

If the module is unresolved, we emit a diagnostic just like for any other unresolved import:

```py
# TODO: not a great error message
from foo import *  # error: [unresolved-import] "Cannot resolve import `foo`"
```

### Nested scope

A `*` import in a nested scope are always a syntax error. Red-knot does not infer any bindings from
them:

`a.py`:

```py
X: bool = True
```

`b.py`:

```py
def f():
    # TODO: we should emit a syntax errror here (tracked by https://github.com/astral-sh/ruff/issues/11934)
    from a import *

    # error: [unresolved-reference]
    reveal_type(X)  # revealed: Unknown
```

[python language reference for import statements]: https://docs.python.org/3/reference/simple_stmts.html#the-import-statement
[typing spec]: https://typing.python.org/en/latest/spec/distributing.html#library-interface-public-and-private-symbols
