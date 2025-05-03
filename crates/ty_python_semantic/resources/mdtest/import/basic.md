# Structures

## Class import following

```py
from b import C as D

E = D
reveal_type(E)  # revealed: Literal[C]
```

`b.py`:

```py
class C: ...
```

## Module member resolution

```py
import b

D = b.C
reveal_type(D)  # revealed: Literal[C]
```

`b.py`:

```py
class C: ...
```

## Nested

```py
import a.b

reveal_type(a.b.C)  # revealed: Literal[C]
```

`a/__init__.py`:

```py
```

`a/b.py`:

```py
class C: ...
```

## Deeply nested

```py
import a.b.c

reveal_type(a.b.c.C)  # revealed: Literal[C]
```

`a/__init__.py`:

```py
```

`a/b/__init__.py`:

```py
```

`a/b/c.py`:

```py
class C: ...
```

## Nested with rename

```py
import a.b as b

reveal_type(b.C)  # revealed: Literal[C]
```

`a/__init__.py`:

```py
```

`a/b.py`:

```py
class C: ...
```

## Deeply nested with rename

```py
import a.b.c as c

reveal_type(c.C)  # revealed: Literal[C]
```

`a/__init__.py`:

```py
```

`a/b/__init__.py`:

```py
```

`a/b/c.py`:

```py
class C: ...
```

## Unresolvable module import

<!-- snapshot-diagnostics -->

```py
import zqzqzqzqzqzqzq  # error: [unresolved-import] "Cannot resolve import `zqzqzqzqzqzqzq`"
```

## Unresolvable submodule imports

<!-- snapshot-diagnostics -->

```py
# Topmost component resolvable, submodule not resolvable:
import a.foo  # error: [unresolved-import] "Cannot resolve import `a.foo`"

# Topmost component unresolvable:
import b.foo  # error: [unresolved-import] "Cannot resolve import `b.foo`"
```

`a/__init__.py`:

```py
```

## Long paths

It's unlikely that a single module component is as long as in this example, but Windows treats paths
that are longer than 200 and something specially. This test ensures that Red Knot can handle those
paths gracefully.

```toml
system = "os"
```

`AveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPath/__init__.py`:

```py
class Foo: ...
```

```py
from AveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPathAveryLongPath import (
    Foo,
)

reveal_type(Foo())  # revealed: Foo
```
