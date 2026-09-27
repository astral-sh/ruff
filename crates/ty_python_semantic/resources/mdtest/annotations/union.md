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

## Runtime union operands

`None` can be combined with classes and type forms in either order:

```py
from typing import Annotated, Callable, Literal, NewType, TypeVar
from typing_extensions import TypeAliasType

T = TypeVar("T")
UserId = NewType("UserId", int)
Alias = TypeAliasType("Alias", int)

reveal_type(int | None)  # revealed: <types.UnionType special-form 'int | None'>
reveal_type(None | list[int])  # revealed: <types.UnionType special-form 'None | list[int]'>
reveal_type(Literal[1] | None)  # revealed: <types.UnionType special-form 'Literal[1] | None'>
reveal_type(None | Annotated[int, "tag"])  # revealed: <types.UnionType special-form 'None | int'>
reveal_type(Callable[[], int] | None)  # revealed: <types.UnionType special-form '(() -> int) | None'>
reveal_type(None | T)  # revealed: <types.UnionType special-form 'None | TypeVar'>
reveal_type(UserId | None)  # revealed: <types.UnionType special-form 'UserId | None'>
reveal_type(None | Alias)  # revealed: <types.UnionType special-form 'None | int'>
```

Ordinary values do not become type union operands merely because their type is known to ty:

```py
from functools import partial

def f(x: int) -> int:
    return x

p = partial(f)
p | None  # error: [unsupported-operator]
None | p  # error: [unsupported-operator]
staticmethod(f) | None  # error: [unsupported-operator]
range(5) | None  # error: [unsupported-operator]
```

## Recursive assignments to invalid union operands

An invalid operation still produces a diagnostic when its operand is inferred recursively:

```py
from functools import partial

def f(x: int) -> int:
    return x

def loop(flag: bool) -> None:
    value = partial(f)
    while flag:
        value = value | None  # error: [unsupported-operator]
    reveal_type(value)  # revealed: partial[(x: int) -> int] | Unknown

def reverse_loop(flag: bool) -> None:
    value = staticmethod(f)
    while flag:
        value = None | value  # error: [unsupported-operator]
    reveal_type(value)  # revealed: staticmethod[def f(x: int) -> int] | Unknown
```

```py
class X:
    def __init__(self):
        self.value = partial(f)

    def update(self):
        self.value = self.value | None  # error: [unsupported-operator]

reveal_type(X().value)  # revealed: partial[(x: int) -> int] | Unknown
```

## Runtime class

### Python 3.13 and earlier

`typing.Union` is an instance of `typing._SpecialForm`, so it is not a class.

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Union

reveal_type(type(Union))  # revealed: <class '_SpecialForm'>

def takes_type(cls: type) -> None: ...

takes_type(Union)  # error: [invalid-argument-type]
```

### Python 3.14 and later

`typing.Union` is a class, as is its re-export from `typing_extensions`.

```toml
[environment]
python-version = "3.14"
```

```py
from typing import Union
from typing_extensions import Union as ExtensionsUnion

reveal_type(type(Union))  # revealed: <class 'type'>
reveal_type(type(ExtensionsUnion))  # revealed: <class 'type'>

def takes_type(cls: type) -> None: ...

takes_type(Union)
takes_type(ExtensionsUnion)
```
