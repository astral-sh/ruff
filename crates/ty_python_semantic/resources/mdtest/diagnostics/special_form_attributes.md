# Diagnostics for invalid attribute access on special forms

<!-- snapshot-diagnostics -->

```py
from typing_extensions import Any, Final, LiteralString, Self

X = Any

class Foo:
    X: Final = LiteralString
    a: int
    b: Self

    class Bar:
        def __init__(self):
            self.y: Final = LiteralString

X.foo  # error: [missing-attribute]
X.aaaaooooooo  # error: [missing-attribute]
Foo.X.startswith  # error: [missing-attribute]
Foo.Bar().y.startswith  # error: [missing-attribute]

# `Foo().b` resolves `Self` to `Foo`, so `.a` is valid.
Foo().b.a
```
