# Regression test for #3020

Regression test for [this issue](https://github.com/astral-sh/ty/issues/3020).

```python
from typing import Callable
from ty_extensions import CallableTypeOf, static_assert, is_assignable_to, is_subtype_of

def f(a: int, b: str, /) -> None: ...

static_assert(is_assignable_to(Callable[[int, str], None], CallableTypeOf[f]))
static_assert(is_subtype_of(Callable[[int, str], None], CallableTypeOf[f]))
```
