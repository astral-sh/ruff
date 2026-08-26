# Function decorator inference cycle

Regression test for <https://github.com/astral-sh/ty/issues/3593>.

```toml
[environment]
python-version = "3.14"
```

```py
from typing import Self, overload, reveal_type

class C:
    a: D

# error: [invalid-attribute-access]
C.a
# error: [invalid-attribute-access]
reveal_type(C().a)  # revealed: Unknown | D

class D:
    @overload
    # error: [invalid-overload]
    # error: [invalid-overload]
    def __get__() -> Self:
        pass
```
