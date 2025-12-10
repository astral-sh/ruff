# Invalid Type Param Order

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.13"
```

```py
from typing import TypeVar, Generic

T1 = TypeVar("T1", default=int)
T2 = TypeVar("T2")
T3 = TypeVar("T3")
DefaultStrT = TypeVar("DefaultStrT", default=str)

class SubclassMe(Generic[T1, DefaultStrT]):
    x: DefaultStrT

class Baz(SubclassMe[int, DefaultStrT]):
    pass

class Foo(Generic[T1, T2]):  # error: [invalid-type-param-order]
    pass

class Bar(Generic[T2, T1, T3]):  # error: [invalid-type-param-order]
    pass
```
