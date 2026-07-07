# Regression test for #3020

Regression test for [this issue](https://github.com/astral-sh/ty/issues/3020).

```python
from typing import Callable
from ty_extensions import static_assert
from ty_extensions._internal import (
    RegularCallableTypeOf,
    TypeOf,
    is_assignable_to,
    is_subtype_of,
    into_regular_callable,
)

def f(a: int, b: str, /) -> None: ...

static_assert(is_assignable_to(Callable[[int, str], None], RegularCallableTypeOf[f]))
static_assert(is_subtype_of(Callable[[int, str], None], RegularCallableTypeOf[f]))
static_assert(is_assignable_to(Callable[[int, str], None], TypeOf[into_regular_callable(f)]))
static_assert(is_subtype_of(Callable[[int, str], None], TypeOf[into_regular_callable(f)]))
```
