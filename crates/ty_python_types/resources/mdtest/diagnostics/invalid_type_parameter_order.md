# Invalid Order of Legacy Type Parameters

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.13"
```

```py
from typing import TypeVar, Generic, Protocol

T1 = TypeVar("T1", default=int)

T2 = TypeVar("T2")
T3 = TypeVar("T3")

DefaultStrT = TypeVar("DefaultStrT", default=str)

class SubclassMe(Generic[T1, DefaultStrT]):
    x: DefaultStrT

class Baz(SubclassMe[int, DefaultStrT]):
    pass

# error: [invalid-generic-class] "Type parameter `T2` without a default cannot follow earlier parameter `T1` with a default"
class Foo(Generic[T1, T2]):
    pass

class Bar(Generic[T2, T1, T3]):  # error: [invalid-generic-class]
    pass

class Spam(Generic[T1, T2, DefaultStrT, T3]):  # error: [invalid-generic-class]
    pass

class Ham(Protocol[T1, T2, DefaultStrT, T3]):  # error: [invalid-generic-class]
    pass

class VeryBad(
    Protocol[T1, T2, DefaultStrT, T3],  # error: [invalid-generic-class]
    Generic[T1, T2, DefaultStrT, T3],
): ...
```
