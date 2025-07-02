# Narrowing using `hasattr()`

The builtin function `hasattr()` can be used to narrow nominal and structural types. This is
accomplished using an intersection with a synthesized protocol:

```py
from typing import final
from typing_extensions import LiteralString

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

def g(x: Bar):
    if hasattr(x, "spam"):
        reveal_type(x)  # revealed: Never
        reveal_type(x.spam)  # revealed: Never
    else:
        reveal_type(x)  # revealed: Bar

        # error: [unresolved-attribute]
        reveal_type(x.spam)  # revealed: Unknown

def returns_bool() -> bool:
    return False

class Baz:
    if returns_bool():
        x: int = 42

def h(obj: Baz):
    reveal_type(obj)  # revealed: Baz
    # error: [possibly-unbound-attribute]
    reveal_type(obj.x)  # revealed: int

    if hasattr(obj, "x"):
        reveal_type(obj)  #  revealed: Baz & <Protocol with members 'x'>
        reveal_type(obj.x)  # revealed: int
    else:
        reveal_type(obj)  # revealed: Baz & ~<Protocol with members 'x'>

        # TODO: should emit `[unresolved-attribute]` and reveal `Unknown`
        reveal_type(obj.x)  # revealed: @Todo(map_with_boundness: intersections with negative contributions)

def i(x: int | LiteralString):
    if hasattr(x, "capitalize"):
        reveal_type(x)  # revealed: (int & <Protocol with members 'capitalize'>) | LiteralString
    else:
        reveal_type(x)  # revealed: int & ~<Protocol with members 'capitalize'>
```
