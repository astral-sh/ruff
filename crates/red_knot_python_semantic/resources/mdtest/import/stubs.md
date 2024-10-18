# Stubs

## Import from stub declaration

```py
from b import x

y = x
reveal_type(y)  # revealed: int
```

```py path=b.pyi
x: int
```

## Import from non-stub with declaration and definition

```py
from b import x

y = x
reveal_type(y)  # revealed: int
```

```py path=b.py
x: int = 1
```
