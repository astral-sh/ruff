# Import module `__file__` symbol behaviour

## Import successful

```py
from b import __file__ as module_path
from stub import __file__ as stub_path
from override import __file__ as overidden_path
from sys import __file__ as no_path

reveal_type(__file__)  # revealed: str
reveal_type(module_path)  # revealed: str
reveal_type(stub_path)  # revealed: str | None
reveal_type(overidden_path)  # revealed: None
reveal_type(no_path)  # revealed: str | None
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
