# Narrowing using `hasattr()`

The builtin function `hasattr()` can be used to narrow nominal and structural types. This is
accomplished using an intersection with a synthesized protocol:

```py
from typing import final

class Foo: ...

@final
class Bar: ...

def f(x: Foo):
    if hasattr(x, "spam"):
        reveal_type(x)  # revealed: Foo & <Protocol with members 'spam'>
        reveal_type(x.spam)  # revealed: object
    else:
        reveal_type(x)  # revealed: Foo & ~<Protocol with members 'spam'>

        # TODO: should error and reveal `Unknown`
        reveal_type(x.spam)  # revealed: @Todo(map_with_boundness: intersections with negative contributions)

    if hasattr(x, "not-an-identifier"):
        reveal_type(x)  # revealed: Foo
    else:
        reveal_type(x)  # revealed: Foo

def y(x: Bar):
    if hasattr(x, "spam"):
        reveal_type(x)  # revealed: Never
        reveal_type(x.spam)  # revealed: Never
    else:
        reveal_type(x)  # revealed: Bar

        # error: [unresolved-attribute]
        reveal_type(x.spam)  # revealed: Unknown
```
