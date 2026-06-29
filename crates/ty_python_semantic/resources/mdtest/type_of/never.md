# `type[Never]`

```py
from typing import Type
from typing_extensions import Never
from ty_extensions import is_equivalent_to, static_assert

static_assert(is_equivalent_to(type[Never], Never))
static_assert(is_equivalent_to(Type[Never], Never))

def f(value: Never):
    reveal_type(type(value))  # revealed: Never
```
