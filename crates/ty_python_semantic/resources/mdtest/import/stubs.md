# Stubs

## Import from stub declaration

```py
from b import x

y = x
reveal_type(y)  # revealed: int
```

`b.pyi`:

```pyi
x: int
```

## Import from non-stub with declaration and definition

```py
from b import x

y = x
reveal_type(y)  # revealed: int
```

`b.py`:

```py
x: int = 1
```

## Typeshed inside the project

Standard-library stubs retain their builtin and typing semantics when the configured typeshed is
inside a first-party search path.

```toml
[environment]
typeshed = "/src/stubs"
```

`/src/stubs/stdlib/builtins.pyi`:

```pyi
class object: ...
class type: ...

class int:
    def __add__(self, other: int, /) -> int: ...

class str: ...
class tuple: ...
```

`/src/stubs/stdlib/typing.pyi`:

```pyi
class _SpecialForm: ...

Literal: _SpecialForm
```

`/src/stubs/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

```py
from typing import Literal

def add(a: int, b: int) -> int:
    return a + b

reveal_type(add(1, 2))  # revealed: int
add(1, "2")  # error: [invalid-argument-type]

value: Literal[1] = 1
reveal_type(value)  # revealed: Literal[1]
```
