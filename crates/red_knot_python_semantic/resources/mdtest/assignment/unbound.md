# Unbound

## Maybe unbound

```py path=package/maybe_unbound.py
if flag:
    y = 3
x = y
reveal_type(x)  # revealed: Unbound | Literal[3]
reveal_type(y)  # revealed: Unbound | Literal[3]
```

```py path=package/public.py
from .maybe_unbound import x, y # error: [possibly-unresolved-import]
reveal_type(x)  # revealed: Literal[3]
reveal_type(y)  # revealed: Literal[3]
```

## Maybe unbound annotated

```py path=package/maybe_unbound_annotated.py
if flag:
    y: int = 3
x = y
reveal_type(x)  # revealed: Unbound | Literal[3]
reveal_type(y)  # revealed: Unbound | int
```

```py path=package/public.py
from .maybe_unbound_annotated import x, y # error: [possibly-unresolved-import]
reveal_type(x)  # revealed: Literal[3]
reveal_type(y)  # revealed: int
```

## Unbound

```py path=unbound/
x = foo; foo = 1
reveal_type(x)  # revealed: Unbound
```

## Unbound class variable

Class variables can reference global variables unless overridden within the class scope.

```py
x = 1
class C:
    y = x
    if flag:
        x = 2

reveal_type(C.x) # revealed: Unbound | Literal[2]
reveal_type(C.y) # revealed: Literal[1]
```
