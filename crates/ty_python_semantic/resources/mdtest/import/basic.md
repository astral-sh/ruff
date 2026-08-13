# Structures

## Class import following

```py
from b import C as D

E = D
reveal_type(E)  # revealed: <class 'C'>
```

`b.py`:

```py
class C: ...
```

## Module member resolution

```py
import b

D = b.C
reveal_type(D)  # revealed: <class 'C'>
```

`b.py`:

```py
class C: ...
```

## Nested

```py
import a.b

reveal_type(a.b.C)  # revealed: <class 'C'>
```

`a/__init__.py`:

```py
```

`a/b.py`:

```py
class C: ...
```

## Deeply nested

```py
import a.b.c

reveal_type(a.b.c.C)  # revealed: <class 'C'>
```

`a/__init__.py`:

```py
```

`a/b/__init__.py`:

```py
```

`a/b/c.py`:

```py
class C: ...
```

## Nested with rename

```py
import a.b as b

reveal_type(b.C)  # revealed: <class 'C'>
```

`a/__init__.py`:

```py
```

`a/b.py`:

```py
class C: ...
```

## Deeply nested with rename

```py
import a.b.c as c

reveal_type(c.C)  # revealed: <class 'C'>
```

`a/__init__.py`:

```py
```

`a/b/__init__.py`:

```py
```

`a/b/c.py`:

```py
class C: ...
```

## Unresolvable module import

<!-- snapshot-diagnostics -->

```py
import zqzqzqzqzqzqzq  # error: [unresolved-import] "Cannot resolve imported module `zqzqzqzqzqzqzq`"
```

## Unresolvable submodule imports

<!-- snapshot-diagnostics -->

```py
# Topmost component resolvable, submodule not resolvable:
import a.foo  # error: [unresolved-import] "Cannot resolve imported module `a.foo`"

# Topmost component unresolvable:
import b.foo  # error: [unresolved-import] "Cannot resolve imported module `b.foo`"
```

`a/__init__.py`:

```py
```

## Long paths

It's unlikely that a single module component is as long as in this example, but Windows treats paths
that are longer than 200 and something specially. This test ensures that ty can handle those paths
gracefully.

```toml
system = "os"
```

`AveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPath/__init__.py`:

```py
class Foo: ...
```

```py
from AveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPath import (
    Foo,
)

reveal_type(Foo())  # revealed: Foo
```

## Multiple objects imported from an unresolved module

<!-- snapshot-diagnostics -->

If multiple members are imported from a module that cannot be resolved, only a single diagnostic is
emitted for the `import from` statement:

```py
# error: [unresolved-import]
from does_not_exist import foo, bar, baz
```

## Attempting to import a stdlib module that's not yet been added

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.10"
```

```py
import tomllib  # error: [unresolved-import]
from string.templatelib import Template  # error: [unresolved-import]
from importlib.resources import abc  # error: [unresolved-import]
```

## Attempting to import a stdlib submodule when both parts haven't yet been added

`compression` and `compression.zstd` were both added in 3.14 so there is a typeshed `VERSIONS` entry
for `compression` but not `compression.zstd`. We can't be confident `compression.zstd` exists but we
do know `compression` does and can still give good diagnostics about it.

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.10"
```

```py
import compression.zstd  # error: [unresolved-import]
from compression import zstd  # error: [unresolved-import]
import compression.fakebutwhocansay  # error: [unresolved-import]
from compression import fakebutwhocansay  # error: [unresolved-import]
```

## Attempting to import a stdlib module that was previously removed

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.13"
```

```py
import aifc  # error: [unresolved-import]
from distutils import sysconfig  # error: [unresolved-import]
```

## Cannot shadow core standard library modules

`types.py`:

```py
x: int
```

```py
# error: [unresolved-import]
from types import x

from types import FunctionType
```
