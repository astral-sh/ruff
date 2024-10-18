# Structures

## Class import following

```py
from b import C as D

E = D
reveal_type(E)  # revealed: Literal[C]
```

```py path=b.py
class C: ...
```

## Module member resolution

```py
import b

D = b.C
reveal_type(D)  # revealed: Literal[C]
```

```py path=b.py
class C: ...
```
