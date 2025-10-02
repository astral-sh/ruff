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
    def make_instance(cls: Type["C"]) -> Self:
        return cls()

    def foo(self) -> Self:
        return self

reveal_type(C.make_instance())  # revealed: Unknown
reveal_type(C.make_instance)  # revealed: bound method <class 'C'>.make_instance() -> C
reveal_type(C().foo())  # revealed: C
```
