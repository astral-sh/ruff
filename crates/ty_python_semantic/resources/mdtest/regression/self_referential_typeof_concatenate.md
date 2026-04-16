# Regression test for self-referential `TypeOf` inside `Callable[Concatenate[...]]`

```toml
[environment]
python-version = "3.14"
```

```py
from collections.abc import Callable
from typing import Concatenate
from ty_extensions import TypeOf, generic_context

def foo[**P, T](
    x: Callable[Concatenate[TypeOf[foo], ...], T],
) -> Callable[Concatenate[TypeOf[foo], P], T]:
    return x

reveal_type(generic_context(foo))  # revealed: ty_extensions.GenericContext[T@foo]
# revealed: def foo[T](x: (def foo(...), /, *args: Any, **kwargs: Any) -> T) -> ((def foo(...), /, *args: P'return.args, **kwargs: P'return.kwargs) -> T)
reveal_type(foo)
```
