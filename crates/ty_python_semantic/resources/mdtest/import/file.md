# Import module `__file__` symbol behaviour

## Import successful

```py
from b import __file__ as module_path
from stub import __file__ as stub_path

reveal_type(__file__)  # revealed: str
reveal_type(module_path)  # revealed: str
reveal_type(stub_path)  # revealed: str | None
```

`b.py`:

```py

```

`stub.pyi`:

```pyi
```

## Import failed

```py
from bar import __file__ as module_path  # error: "Cannot resolve imported module `bar`"

reveal_type(module_path)  # revealed: Unknown
```
