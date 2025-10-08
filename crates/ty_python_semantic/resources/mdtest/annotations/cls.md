# `cls`

```toml
[environment]
python-version = "3.13"
```

## Methods

```py
from typing import Type, Self

class C:
    @classmethod
    def make_instance(cls: Type[Self]) -> Self:
        return cls()

reveal_type(C.make_instance())  # revealed: C
reveal_type(C.make_instance)  # revealed: bound method <class 'C'>.make_instance() -> C
```
