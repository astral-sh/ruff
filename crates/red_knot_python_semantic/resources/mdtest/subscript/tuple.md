# Tuple subscripts

## Indexing

```py
t = (1, "a", "b")

reveal_type(t[0])  # revealed: Literal[1]
reveal_type(t[1])  # revealed: Literal["a"]
reveal_type(t[-1])  # revealed: Literal["b"]
reveal_type(t[-2])  # revealed: Literal["a"]

reveal_type(t[False])  # revealed: Literal[1]
reveal_type(t[True])  # revealed: Literal["a"]

a = t[4]  # error: [index-out-of-bounds]
reveal_type(a)  # revealed: Unknown

b = t[-4]  # error: [index-out-of-bounds]
reveal_type(b)  # revealed: Unknown
```

## Slices

```py
t = (1, "a", None, b"b")

reveal_type(t[0:0])  # revealed: tuple[()]
reveal_type(t[0:1])  # revealed: tuple[Literal[1]]
reveal_type(t[0:2])  # revealed: tuple[Literal[1], Literal["a"]]
reveal_type(t[0:4])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]
reveal_type(t[0:5])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]
reveal_type(t[1:3])  # revealed: tuple[Literal["a"], None]

reveal_type(t[-2:4])  # revealed: tuple[None, Literal[b"b"]]
reveal_type(t[-3:-1])  # revealed: tuple[Literal["a"], None]
reveal_type(t[-10:10])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]

reveal_type(t[0:])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]
reveal_type(t[2:])  # revealed: tuple[None, Literal[b"b"]]
reveal_type(t[4:])  # revealed: tuple[()]
reveal_type(t[:0])  # revealed: tuple[()]
reveal_type(t[:2])  # revealed: tuple[Literal[1], Literal["a"]]
reveal_type(t[:10])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]
reveal_type(t[:])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]

reveal_type(t[::-1])  # revealed: tuple[Literal[b"b"], None, Literal["a"], Literal[1]]
reveal_type(t[::2])  # revealed: tuple[Literal[1], None]
reveal_type(t[-2:-5:-1])  # revealed: tuple[None, Literal["a"], Literal[1]]
reveal_type(t[::-2])  # revealed: tuple[Literal[b"b"], Literal["a"]]
reveal_type(t[-1::-3])  # revealed: tuple[Literal[b"b"], Literal[1]]

reveal_type(t[None:2:None])  # revealed: tuple[Literal[1], Literal["a"]]
reveal_type(t[1:None:1])  # revealed: tuple[Literal["a"], None, Literal[b"b"]]
reveal_type(t[None:None:None])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]

start = 1
stop = None
step = 2
reveal_type(t[start:stop:step])  # revealed: tuple[Literal["a"], Literal[b"b"]]

reveal_type(t[False:True])  # revealed: tuple[Literal[1]]
reveal_type(t[True:3])  # revealed: tuple[Literal["a"], None]

t[0:4:0]  # error: [zero-stepsize-in-slice]
t[:4:0]  # error: [zero-stepsize-in-slice]
t[0::0]  # error: [zero-stepsize-in-slice]
t[::0]  # error: [zero-stepsize-in-slice]

def _(m: int, n: int):
    tuple_slice = t[m:n]
    # TODO: Support overloads... Should be `tuple[Literal[1, 'a', b"b"] | None, ...]`
    reveal_type(tuple_slice)  # revealed: @Todo(return type)
```

## Inheritance

```toml
[environment]
python-version = "3.9"
```

```py
# TODO:
#  * `tuple.__class_getitem__` is always bound on 3.9 (`sys.version_info`)
#  * `tuple[int, str]` is a valid base (generics)
# error: [call-possibly-unbound-method] "Method `__class_getitem__` of type `Literal[tuple]` is possibly unbound"
# error: [invalid-base] "Invalid class base with type `GenericAlias` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
class A(tuple[int, str]): ...

# Runtime value: `(A, tuple, object)`
# TODO: Generics
reveal_type(A.__mro__)  # revealed: tuple[Literal[A], Unknown, Literal[object]]
```

## `typing.Tuple`

### Correspondence with `tuple`

`typing.Tuple` can be used interchangeably with `tuple`:

```py
from typing import Tuple

class A: ...

def _(c: Tuple, d: Tuple[int, A], e: Tuple[Any, ...]):
    reveal_type(c)  # revealed: tuple
    reveal_type(d)  # revealed: tuple[int, A]
    reveal_type(e)  # revealed: @Todo(full tuple[...] support)
```

### Inheritance

Inheriting from `Tuple` results in a MRO with `builtins.tuple` and `typing.Generic`. `Tuple` itself
is not a class.

```py
from typing import Tuple

class C(Tuple): ...

# Runtime value: `(C, tuple, typing.Generic, object)`
# TODO: Add `Generic` to the MRO
reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[tuple], Unknown, Literal[object]]
```
