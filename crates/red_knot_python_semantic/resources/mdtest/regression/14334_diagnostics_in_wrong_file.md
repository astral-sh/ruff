# Regression test for #14334

Regression test for [this issue](https://github.com/astral-sh/ruff/issues/14334).

```py path=base.py
# error: [invalid-base]
class Base(2): ...
```

```py path=a.py
# No error here
from base import Base
```
