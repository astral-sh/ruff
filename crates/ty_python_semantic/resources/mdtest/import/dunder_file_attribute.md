# Import module `__file__` symbol behaviour

## Import successful

```py
from b import __file__ as module_path
from stub import __file__ as stub_path
from override import __file__ as overidden_path
from sys import __file__ as no_path

reveal_type(__file__)  # revealed: str
reveal_type(module_path)  # revealed: str
reveal_type(stub_path)  # revealed: str
reveal_type(overidden_path)  # revealed: None
# NOTE: This is `None` at runtime as this is a statically linked C Extension
#       but this behaviour matches other typecheckers in not
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

## Import failed

```py
from bar import __file__ as module_path  # error: "Cannot resolve imported module `bar`"

reveal_type(module_path)  # revealed: Unknown
```

## `__init__.py` does have `__file__`

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

## Namespace package does not have `__file__`

`namespace/c.py`:

```py
```

```py
import namespace

# error: [unresolved-attribute] "Module `namespace` has no member `__file__`"
reveal_type(namespace.__file__)  # revealed: Unknown
```
