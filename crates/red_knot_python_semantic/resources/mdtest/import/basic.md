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

## Nested

```py
import a.b
reveal_type(a.b.C)  # revealed: Literal[C]
```

```py path=a/__init__.py
```

```py path=a/b.py
class C: ...
```

## Deeply nested

```py
import a.b.c
reveal_type(a.b.c.C)  # revealed: Literal[C]
```

```py path=a/__init__.py
```

```py path=a/b/__init__.py
```

```py path=a/b/c.py
class C: ...
```
