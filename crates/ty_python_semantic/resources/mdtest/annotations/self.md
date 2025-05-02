# Self

## Methods

`Self` used in the signature of a method is treated as if it were a `TypeVar` bound to the class.

`typing.Self` is only available in Python 3.11 and later.

```toml
[environment]
python-version = "3.11"
```

```py
from typing import Self

class A:
  def __new__(cls: Self) -> Self:
    reveal_type(cls) # revealed: A
    return cls
```
