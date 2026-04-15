# Union

## Annotation

`typing.Union` can be used to construct union types in the same way as the `|` operator.

```py
from typing import Union

a: Union[int, str]
a1: Union[int, bool]
a2: Union[int, Union[bytes, str]]
a3: Union[int, None]
a4: Union[Union[bytes, str]]
a5: Union[int]
a6: Union[()]

def f():
    # revealed: int | str
    reveal_type(a)
    # Since bool is a subtype of int we simplify to int here. But we do allow assigning boolean values (see below).
    # revealed: int
    reveal_type(a1)
    # revealed: int | bytes | str
    reveal_type(a2)
    # revealed: int | None
    reveal_type(a3)
    # revealed: bytes | str
    reveal_type(a4)
    # revealed: int
    reveal_type(a5)
    # revealed: Never
    reveal_type(a6)
```

## Assignment

```py
from typing import Union

a: Union[int, str]
a = 1
a = ""
a1: Union[int, bool]
a1 = 1
a1 = True
# error: [invalid-assignment] "Object of type `Literal[b""]` is not assignable to `int | str`"
a = b""
```

## Typing Extensions

```py
from typing_extensions import Union

a: Union[int, str]

def f():
    # revealed: int | str
    reveal_type(a)
```

## Invalid

```py
from typing import Union

# error: [invalid-type-form] "`typing.Union` requires at least one argument when used in a parameter annotation"
def f(x: Union) -> None:
    reveal_type(x)  # revealed: Unknown
```

## Implicit type aliases using new-style unions

```toml
[environment]
python-version = "3.10"
```

```py
X = int | str

def f(y: X):
    reveal_type(y)  # revealed: int | str
```

## Diagnostics for PEP-604 unions used on Python less than 3.10

PEP-604 unions generally don't work on Python 3.9 and earlier:

```toml
[environment]
python-version = "3.9"
```

`a.py`:

```py
x: int | str  # snapshot: unsupported-operator

class Foo:
    def __init__(self):
        self.x: int | str = 42  # snapshot: unsupported-operator

d = {}
d[0]: int | str = 42  # snapshot: unsupported-operator
```

```snapshot
error[unsupported-operator]: Unsupported `|` operation
 --> src/a.py:1:4
  |
1 | x: int | str  # snapshot: unsupported-operator
  |    ---^^^---
  |    |     |
  |    |     Has type `<class 'str'>`
  |    Has type `<class 'int'>`
  |
info: PEP 604 `|` unions are only available on Python 3.10+ unless they are quoted
info: Python 3.9 was assumed when resolving types because it was specified on the command line


error[unsupported-operator]: Unsupported `|` operation
 --> src/a.py:5:17
  |
5 |         self.x: int | str = 42  # snapshot: unsupported-operator
  |                 ---^^^---
  |                 |     |
  |                 |     Has type `<class 'str'>`
  |                 Has type `<class 'int'>`
  |
info: PEP 604 `|` unions are only available on Python 3.10+ unless they are quoted
info: Python 3.9 was assumed when resolving types because it was specified on the command line


error[unsupported-operator]: Unsupported `|` operation
 --> src/a.py:8:7
  |
8 | d[0]: int | str = 42  # snapshot: unsupported-operator
  |       ---^^^---
  |       |     |
  |       |     Has type `<class 'str'>`
  |       Has type `<class 'int'>`
  |
info: PEP 604 `|` unions are only available on Python 3.10+ unless they are quoted
info: Python 3.9 was assumed when resolving types because it was specified on the command line
```

But these runtime errors can be avoided if you add `from __future__ import annotations` to the top
of your file:

`b.py`:

```py
from __future__ import annotations

x: int | str

class Foo:
    def __init__(self):
        self.x: int | str = 42

d = {}
d[0]: int | str = 42
```

The following ones are still errors because `from __future__ import annotations` only stringifies
*type annotations*, not arbitrary runtime expressions:

`c.py`:

```py
X = str | int  # snapshot: unsupported-operator
Y = tuple[str | int, ...]  # snapshot: unsupported-operator
```

```snapshot
error[unsupported-operator]: Unsupported `|` operation
 --> src/c.py:1:5
  |
1 | X = str | int  # snapshot: unsupported-operator
  |     ---^^^---
  |     |     |
  |     |     Has type `<class 'int'>`
  |     Has type `<class 'str'>`
  |
info: PEP 604 `|` unions are only available on Python 3.10+ unless they are quoted
info: Python 3.9 was assumed when resolving types because it was specified on the command line


error[unsupported-operator]: Unsupported `|` operation
 --> src/c.py:2:11
  |
2 | Y = tuple[str | int, ...]  # snapshot: unsupported-operator
  |           ---^^^---
  |           |     |
  |           |     Has type `<class 'int'>`
  |           Has type `<class 'str'>`
  |
info: PEP 604 `|` unions are only available on Python 3.10+ unless they are quoted
info: Python 3.9 was assumed when resolving types because it was specified on the command line
```
