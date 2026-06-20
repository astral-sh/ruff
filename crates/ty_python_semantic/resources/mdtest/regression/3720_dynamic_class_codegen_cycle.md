# Regression test for #3720

Regression test for [this issue](https://github.com/astral-sh/ty/issues/3720).

```toml
[environment]
python-version = "3.12"
```

```py
from abc import ABC
from typing import NamedTuple

# error: [invalid-argument-type] "Invalid argument to parameter 2 (`bases`) of `type()`"
# error: [invalid-named-tuple] "is not a valid identifier"
T = type("T", NamedTuple("T", [("", "T")]), {})
T()
T = ABC
```
