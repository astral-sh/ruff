# Call expression

## Simple

```py
def get_int() -> int:
    return 42

x = get_int()
reveal_type(x)  # revealed: int
```

## Async

```py
async def get_int_async() -> int:
    return 42

x = get_int_async()

# TODO: we don't yet support `types.CoroutineType`, should be generic `Coroutine[Any, Any, int]`
reveal_type(x)  # revealed: @Todo
```

## Decorated

```py
from typing import Callable

def foo() -> int:
    return 42

def decorator(func) -> Callable[[], int]:
    return foo

@decorator
def bar() -> str:
    return 'bar'

x = bar()

# TODO: should reveal `int`, as the decorator replaces `bar` with `foo`
reveal_type(x)  # revealed: @Todo
```

## Invalid callable

```py
nonsense = 123
x = nonsense()  # error: "Object of type `Literal[123]` is not callable"
```
