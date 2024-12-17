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

## Nested with rename

```py
import a.b as b

reveal_type(b.C)  # revealed: Literal[C]
```

```py path=a/__init__.py
```

```py path=a/b.py
class C: ...
```

## Deeply nested with rename

```py
import a.b.c as c

reveal_type(c.C)  # revealed: Literal[C]
```

```py path=a/__init__.py
```

```py path=a/b/__init__.py
```

```py path=a/b/c.py
class C: ...
```

## Unresolvable submodule imports

```py
# Topmost component resolvable, submodule not resolvable:
import a.foo  # error: [unresolved-import] "Cannot resolve import `a.foo`"

# Topmost component unresolvable:
import b.foo  # error: [unresolved-import] "Cannot resolve import `b.foo`"
```

```py path=a/__init__.py
```
