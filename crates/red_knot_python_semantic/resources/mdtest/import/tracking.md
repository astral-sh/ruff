# Tracking imported modules

These tests depend on how we track which modules have been imported. There are currently two
characteristics of our module tracking that can lead to inaccuracies:

- Imports are tracked on a per-file basis. At runtime, importing a submodule in one file makes that
    submodule globally available via any reference to the containing package. We will flag an error
    if a file tries to access a submodule without there being an import of that submodule _in that
    same file_.

    This is a purposeful decision, and not one we plan to change. If a module wants to re-export some
    other module that it imports, there are ways to do that (tested below) that are blessed by the
    typing spec and that are visible to our file-scoped import tracking.

- Imports are tracked flow-insensitively: submodule accesses are allowed and resolved if that
    submodule is imported _anywhere in the file_. This handles the common case where all imports are
    grouped at the top of the file, and is easiest to implement. We might revisit this decision and
    track submodule imports flow-sensitively, in which case we will have to update the assertions in
    some of these tests.

## Import submodule later in file

This test highlights our flow-insensitive analysis, since we access the `a.b` submodule before it
has been imported.

```py
import a

# Would be an error with flow-sensitive tracking
reveal_type(a.b.C)  # revealed: Literal[C]

import a.b
```

`a/__init__.py`:

```py
```

`a/b.py`:

```py
class C: ...
```

## Rename a re-export

This test highlights how import tracking is local to each file, but specifically to the file where a
containing module is first referenced. This allows the main module to see that `q.a` contains a
submodule `b`, even though `a.b` is never imported in the main module.

```py
from q import a, b

reveal_type(b)  # revealed: <module 'a.b'>
reveal_type(b.C)  # revealed: Literal[C]

reveal_type(a.b)  # revealed: <module 'a.b'>
reveal_type(a.b.C)  # revealed: Literal[C]
```

`a/__init__.py`:

```py
```

`a/b.py`:

```py
class C: ...
```

`q.py`:

```py
import a as a
import a.b as b
```

## Attribute overrides submodule

Technically, either a submodule or a non-module attribute could shadow the other, depending on the
ordering of when the submodule is loaded relative to the parent module's `__init__.py` file being
evaluated. We have chosen to always have the submodule take priority. (This matches pyright's
current behavior, and opposite of mypy's current behavior.)

```py
import sub.b
import attr.b

# In the Python interpreter, `attr.b` is Literal[1]
reveal_type(sub.b)  # revealed: <module 'sub.b'>
reveal_type(attr.b)  # revealed: <module 'attr.b'>
```

`sub/__init__.py`:

```py
b = 1
```

`sub/b.py`:

```py
```

`attr/__init__.py`:

```py
from . import b as _

b = 1
```

`attr/b.py`:

```py
```
