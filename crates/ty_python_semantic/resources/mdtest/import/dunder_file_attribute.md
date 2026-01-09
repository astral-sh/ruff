# The `__file__` attribute on imported modules

## Module successfully resolved

```py
from b import __file__ as module_path
from stub import __file__ as stub_path
from override import __file__ as overidden_path

reveal_type(__file__)  # revealed: str
reveal_type(module_path)  # revealed: str
reveal_type(stub_path)  # revealed: str
reveal_type(overidden_path)  # revealed: None

# NOTE: This import fails at runtime as this is a C Extension
# with no `__file__` global. It's hard for us to determine this
# right now, however (all we know is it comes from a stub file),
# and if it did exist then it would be of type `str` since `sys`
# is not a namespace packages. This behaviour also matches other
# type checkers.
from sys import __file__ as no_path

reveal_type(no_path)  # revealed: str
```

`b.py`:

```py

```

`override.py`:

```py
__file__ = None
```

`stub.pyi`:

```pyi
```

## Module resolution failed

```py
from bar import __file__ as module_path  # error: "Cannot resolve imported module `bar`"

reveal_type(module_path)  # revealed: Unknown
```

## Non-namespace packages have `__file__` available

`a/__init__.py`:

```py
```

`a/b.py`:

```py
```

```py
import a

reveal_type(a.__file__)  # revealed: str
```

## `__file__` is set to `None` for namespace packages

`namespace/c.py`:

```py
```

```py
import namespace

reveal_type(namespace.__file__)  # revealed: None
```
