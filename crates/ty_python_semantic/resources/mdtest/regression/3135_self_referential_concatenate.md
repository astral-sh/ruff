# Regression test for #3135

Regression test for [this issue](https://github.com/astral-sh/ty/issues/3135).

```toml
[environment]
python-version = "3.12"
```

```python
from __future__ import annotations

from collections.abc import Callable
from typing import Concatenate

from ty_extensions import TypeOf


def foo[**P, T](
    x: Callable[Concatenate[TypeOf[foo], ...], T],
) -> Callable[Concatenate[TypeOf[foo], P], T]:
    return x
```
