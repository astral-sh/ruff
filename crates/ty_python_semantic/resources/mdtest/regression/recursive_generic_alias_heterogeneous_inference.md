# Recursive Generic Alias Heterogeneous Inference

```toml
[environment]
python-version = "3.10"
```

```py
from collections.abc import Callable, Sequence
from typing import Generic
from typing_extensions import TypeAliasType, TypeVar, assert_type

T = TypeVar("T", covariant=True, default=str)

class Box(Generic[T]):
    def __init__(self, value: type[T] | Callable[..., T]):
        self.value = value

Item = TypeAliasType(
    "Item",
    type[T] | Callable[..., T] | Box[T],
    type_params=(T,),
)

Spec = TypeAliasType(
    "Spec",
    Item[T] | Sequence["Spec[T]"],
    type_params=(T,),
)

class C(Generic[T]):
    def __init__(self, x: Spec[T] = str) -> None:
        ...

class A: ...
class B: ...

def f() -> int:
    return 1

c = C([str, A, B, Box(f)])
assert_type(c, C[str | A | B | int])
```
