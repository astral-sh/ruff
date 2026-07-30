# Structures

## Imported modules can be reassigned

Importing a module binds its current module-literal type, but does not declare that later
assignments must preserve that exact type.

```py
import os
from types import ModuleType

def normalize(module: ModuleType) -> ModuleType:
    return module

os = normalize(os)
reveal_type(os)  # revealed: ModuleType
```

## Imported names can be rebound by context managers

An imported name is an ordinary binding, so a context-manager target may replace it with a value of
another type.

`values.py`:

```py
value: str = "before"
```

```py
from values import value

class Manager:
    def __enter__(self) -> int:
        return 1

    def __exit__(self, exc_type, exc_value, traceback): ...

with Manager() as value:
    reveal_type(value)  # revealed: int
```

## Existing annotations constrain imported bindings

An import must respect an existing declaration without creating a conflicting declaration of its
own.

`values.py`:

```py
value: str = "available"
```

```py
value: str | None

try:
    from values import value
except ImportError:
    value = None

reveal_type(value)  # revealed: str | None
```

## Imported values must satisfy existing annotations

Although an import does not declare a type, its value must still be assignable to an existing local
declaration.

`values.py`:

```py
value: str = "incompatible"
```

```py
value: int

from values import value  # error: [invalid-assignment]

reveal_type(value)  # revealed: int
```

## Star-imported names can be reassigned

Wildcard imports bind the names they introduce without declaring their imported types.

`values.py`:

```py
value: str = "before"
```

```py
from values import *

value = 1
reveal_type(value)  # revealed: Literal[1]
```

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
