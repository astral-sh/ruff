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
    track imports flow-sensitively, in which case we will have to update the assertions in some of
    these tests.

## Import submodule later in file

This test highlights our flow-insensitive analysis, since we access the `a.b` submodule before it
has been imported.

```py
import a

# Would be an error with flow-sensitive tracking
reveal_type(a.b.C)  # revealed: Literal[C]

import a.b
```

```py path=a/__init__.py
```

```py path=a/b.py
class C: ...
```

## Rename a re-export

This test highlights how import tracking is local to each file, but specifically to the file where a
containing module is first reference. This allows the main module to see that `q.a` contains a
submodule `b`, even though `a.b` is never imported in the main module.

```py
from q import a, b

reveal_type(b)  # revealed: <module 'a.b'>
reveal_type(b.C)  # revealed: Literal[C]

reveal_type(a.b)  # revealed: <module 'a.b'>
reveal_type(a.b.C)  # revealed: Literal[C]
```

```py path=a/__init__.py
```

```py path=a/b.py
class C: ...
```

```py path=q.py
import a as a
import a.b as b
```
