# Diagnostics for invalid attribute access on special forms

<!-- snapshot-diagnostics -->

```py
from typing_extensions import Any, Final, LiteralString

X = Any

class Foo:
    X: Final = LiteralString

    class Bar:
        def __init__(self):
            self.y: Final = LiteralString

X.foo  # error: [unresolved-attribute]
X.aaaaooooooo  # error: [unresolved-attribute]
Foo.X.startswith  # error: [unresolved-attribute]
Foo.Bar().y.startswith  # error: [unresolved-attribute]
```
