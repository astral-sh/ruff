# Regression test for #14334

Regression test for [this issue](https://github.com/astral-sh/ruff/issues/14334).

`base.py`:

```py
# error: [invalid-base]
class Base(2): ...
```

`a.py`:

```py
# No error here
from base import Base
```
