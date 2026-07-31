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

# error: [invalid-attribute-access] "Invalid access to descriptor attribute `a` on type `<class 'C'>`"
C.a
# error: [invalid-attribute-access] "Invalid access to descriptor attribute `a` on type `C`"
reveal_type(C().a)  # revealed: Unknown | D

class D:
    @overload
    # error: [invalid-overload]
    # error: [invalid-overload]
    def __get__() -> Self:
        pass
```
