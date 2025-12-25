# Stubs

## Import from stub declaration

```py
from b import x

y = x
reveal_type(y)  # revealed: int
```

`b.pyi`:

```pyi
x: int
```

## Import from non-stub with declaration and definition

```py
from b import x

y = x
reveal_type(y)  # revealed: int
```

`b.py`:

```py
x: int = 1
```
