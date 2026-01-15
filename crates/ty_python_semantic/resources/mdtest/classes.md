# Class definitions

## Deferred resolution of bases

### Only the stringified name is deferred

If a class base contains a stringified name, only that name is deferred. Other names are resolved
normally.

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import reveal_mro

A = int

class G[T]: ...
class C(A, G["B"]): ...

A = str
B = bytes

reveal_mro(C)  # revealed: (<class 'C'>, <class 'int'>, <class 'G[bytes]'>, typing.Generic, <class 'object'>)
```

## Starred bases

Fixed-length tuples are unpacked when used as starred base classes:

```py
from ty_extensions import reveal_mro

class A: ...
class B: ...
class C: ...

bases = (A, B, C)

class Foo(*bases): ...

# revealed: (<class 'Foo'>, <class 'A'>, <class 'B'>, <class 'C'>, <class 'object'>)
reveal_mro(Foo)
```

Variable-length tuples cannot be unpacked, so they fall back to `Unknown`:

```py
from ty_extensions import reveal_mro

def get_bases() -> tuple[type, ...]:
    return (int, str)

# error: [unsupported-base] "Unsupported class base"
class Bar(*get_bases()): ...

# revealed: (<class 'Bar'>, Unknown, <class 'object'>)
reveal_mro(Bar)
```
