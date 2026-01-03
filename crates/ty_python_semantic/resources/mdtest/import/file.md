# Import module `__file__` symbol behaviour

## Import successful

```py
from b import __file__ as module_path

reveal_type(__file__)  # revealed: str
reveal_type(module_path)  # revealed: str
```

`b.py`:

```py

```

## Import failed

```py
from bar import __file__ as module_path  # error: "Cannot resolve imported module `bar`"

reveal_type(module_path)  # revealed: Unknown
```
