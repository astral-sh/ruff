# Non-inferable constraint projection to a terminal

The call to `cast_to_call` preserves the outer `T` from `wait`. When inferring the inner `T`, that
outer type variable is non-inferable. Projecting its constraint out of the constraint set produces
the `always` terminal. That terminal must be recognized before enumerating the remaining paths;
otherwise, the empty path list is interpreted as unsatisfiable and the inferred specialization
degrades to `Unknown`.

```toml
[environment]
python-version = "3.11"
```

```py
from collections.abc import Awaitable
from typing import Callable, Generic, TypeVar

T_co = TypeVar("T_co", covariant=True)
T = TypeVar("T")

class Call(Generic[T_co]):
    def __call__(self) -> T_co | Awaitable[T_co]:
        raise NotImplementedError

    def result(self) -> T_co:
        raise NotImplementedError

def cast_to_call(value: Callable[[], T | Awaitable[T]] | Call[T]) -> Call[T]:
    raise NotImplementedError

def wait(value: Callable[[], T] | Call[T]) -> T:
    call = cast_to_call(value)
    reveal_type(call)  # revealed: Call[T@wait]
    return call.result()
```
