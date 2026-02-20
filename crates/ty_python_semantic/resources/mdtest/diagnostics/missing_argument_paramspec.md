# Missing argument for ParamSpec

<!-- snapshot-diagnostics -->

For `ParamSpec` callables, both `*args` and `**kwargs` are required since the underlying callable's
signature is unknown. We add a sub-diagnostic explaining why these parameters are required.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Callable

def decorator[**P](func: Callable[P, int]) -> Callable[P, None]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> None:
        func()  # error: [missing-argument]
        func(*args)  # error: [missing-argument]
        func(**kwargs)  # error: [missing-argument]
    return wrapper
```
