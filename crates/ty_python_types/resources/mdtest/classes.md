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
