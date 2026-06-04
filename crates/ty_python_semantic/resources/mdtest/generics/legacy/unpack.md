# Legacy `typing.Unpack`

```toml
[environment]
python-version = "3.11"
```

`Unpack[Ts]` is the legacy spelling of `*Ts`. These tests mirror `TypeVarTuple` use-site scenarios
to ensure that both spellings have the same semantics.

## Generic Classes

### Explicit specialization

```py
from typing import Generic, TypeVar, TypeVarTuple, Unpack

T = TypeVar("T")
Ts = TypeVarTuple("Ts")
U = TypeVar("U")

class Array(Generic[Unpack[Ts]]):
    def shape(self) -> tuple[Unpack[Ts]]:
        raise NotImplementedError

class Sandwich(Generic[T, Unpack[Ts], U]):
    def parts(self) -> tuple[T, Unpack[Ts], U]:
        raise NotImplementedError

def f(
    array: Array[int, str],
    empty: Array[()],
    sandwich1: Sandwich[int, bool, str],
    sandwich2: Sandwich[int, str],
) -> None:
    reveal_type(array)  # revealed: Array[int, str]
    reveal_type(array.shape())  # revealed: tuple[int, str]
    reveal_type(empty)  # revealed: Array[()]
    reveal_type(empty.shape())  # revealed: tuple[()]
    reveal_type(sandwich1.parts())  # revealed: tuple[int, bool, str]
    reveal_type(sandwich2.parts())  # revealed: tuple[int, str]
```

### Inferred specialization from construction

Calling a generic class without explicit type arguments infers its specialization from the
constructor arguments.

```py
from typing import Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

class Array(Generic[Unpack[Ts]]):
    def __init__(self, shape: tuple[Unpack[Ts]]) -> None:
        self.shape = shape

def f(i: int, s: str) -> None:
    reveal_type(Array((i, s)))  # revealed: Array[int, str]
    reveal_type(Array(()))  # revealed: Array[()]
```

### Default type arguments

A defaulted type variable tuple also applies when the class uses the legacy `Unpack` spelling.
Explicit type arguments override the default.

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts", default=Unpack[tuple[int, str]])

class Array(Generic[Unpack[Ts]]):
    def shape(self) -> tuple[Unpack[Ts]]:
        raise NotImplementedError

def f(default: Array, explicit: Array[bool]) -> None:
    reveal_type(default)  # revealed: Array[int, str]
    reveal_type(default.shape())  # revealed: tuple[int, str]
    reveal_type(explicit)  # revealed: Array[bool]
    reveal_type(explicit.shape())  # revealed: tuple[bool]
    reveal_type(Array())  # revealed: Array[int, str]
    reveal_type(Array[bytes, bool]())  # revealed: Array[bytes, bool]
```

### Unspecified type arguments

An unsubscripted variadic generic behaves as if it used an unknown-length tuple of `Any` arguments.
ty represents the missing type information as `Unknown`, distinguishing it from explicitly provided
`Any`.

```py
from typing import Any, Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

class Shelf(Generic[Unpack[Ts]]):
    def contents(self) -> tuple[Unpack[Ts]]:
        raise NotImplementedError

def needs_two_items(x: Shelf[int, str]) -> None:
    raise NotImplementedError

def needs_dynamic_items(x: Shelf) -> None:
    raise NotImplementedError

def f(
    dynamic: Shelf,
    explicitly_dynamic: Shelf[Unpack[tuple[Any, ...]]],
    fixed: Shelf[int, str],
) -> None:
    reveal_type(dynamic)  # revealed: Shelf[*tuple[Unknown, ...]]
    reveal_type(dynamic.contents())  # revealed: tuple[Unknown, ...]
    reveal_type(explicitly_dynamic)  # revealed: Shelf[*tuple[Any, ...]]
    reveal_type(explicitly_dynamic.contents())  # revealed: tuple[Any, ...]
    needs_two_items(dynamic)
    needs_dynamic_items(fixed)
```

## Functions

### Tuple arguments and returns

```py
from typing import TypeVar, TypeVarTuple, Unpack

T = TypeVar("T")
Ts = TypeVarTuple("Ts")
U = TypeVar("U")

def echo(x: tuple[Unpack[Ts]]) -> tuple[Unpack[Ts]]:
    return x

def with_prefix(x: T, y: tuple[Unpack[Ts]]) -> tuple[T, Unpack[Ts]]:
    raise NotImplementedError

def with_suffix(x: tuple[Unpack[Ts]], y: U) -> tuple[Unpack[Ts], U]:
    raise NotImplementedError

def with_both(x: tuple[T, Unpack[Ts], U]) -> tuple[Unpack[Ts]]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(echo((i, s)))  # revealed: tuple[int, str]
    reveal_type(echo(()))  # revealed: tuple[()]
    reveal_type(with_prefix(i, (s, b)))  # revealed: tuple[int, str, bool]
    reveal_type(with_suffix((i, s), b))  # revealed: tuple[int, str, bool]
    reveal_type(with_both((i, s, b)))  # revealed: tuple[str]
```

### Variadic and callable parameters

```py
from typing import Callable, TypeVar, TypeVarTuple, Unpack

T = TypeVar("T")
Ts = TypeVarTuple("Ts")

def args_to_tuple(*args: Unpack[Ts]) -> tuple[Unpack[Ts]]:
    reveal_type(args)  # revealed: tuple[*Ts@args_to_tuple]
    raise NotImplementedError

def first_and_rest(first: T, *rest: Unpack[Ts]) -> tuple[T, Unpack[Ts]]:
    raise NotImplementedError

def call_with_args(func: Callable[[Unpack[Ts]], T], *args: Unpack[Ts]) -> T:
    raise NotImplementedError

def takes_int_str(i: int, s: str) -> bool:
    return True

def callback(callback: Callable[[int, Unpack[Ts]], tuple[Unpack[Ts]]]) -> None:
    reveal_type(callback)  # revealed: (int, /, *Ts@callback) -> tuple[*Ts@callback]

def f(i: int, s: str, b: bool) -> None:
    reveal_type(args_to_tuple(i, s))  # revealed: tuple[int, str]
    reveal_type(args_to_tuple())  # revealed: tuple[()]
    reveal_type(first_and_rest(i, s, b))  # revealed: tuple[int, str, bool]
    reveal_type(call_with_args(takes_int_str, i, s))  # revealed: bool
    call_with_args(takes_int_str, s, i)  # error: [invalid-argument-type]
```

### Length-sensitive inference

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

def same_shape(x: tuple[Unpack[Ts]], y: tuple[Unpack[Ts]]) -> tuple[Unpack[Ts]]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(same_shape((i, s), (b, i)))  # revealed: tuple[int, str | int]
    same_shape((i,), (s, b))  # error: [invalid-argument-type]
```

## Type concatenation

A type variable tuple can be combined with fixed leading or trailing types when using the legacy
spelling.

```py
from typing import Generic, TypeVar, TypeVarTuple, Unpack

T = TypeVar("T")
Shape = TypeVarTuple("Shape")

class Array(Generic[Unpack[Shape]]): ...
class Batch: ...
class Channels: ...
class Height: ...
class Width: ...

def add_batch_axis(x: Array[Unpack[Shape]]) -> Array[Batch, Unpack[Shape]]:
    raise NotImplementedError

def del_batch_axis(x: Array[Batch, Unpack[Shape]]) -> Array[Unpack[Shape]]:
    raise NotImplementedError

def add_batch_channels(x: Array[Unpack[Shape]]) -> Array[Batch, Unpack[Shape], Channels]:
    raise NotImplementedError

def del_channels_axis(x: Array[Unpack[Shape], Channels]) -> Array[Unpack[Shape]]:
    raise NotImplementedError

def prefix_tuple(x: T, y: tuple[Unpack[Shape]]) -> tuple[T, Unpack[Shape]]:
    raise NotImplementedError

def f(a: Array[Height, Width], c: Array[Height, Width, Channels]) -> None:
    b = add_batch_axis(a)
    reveal_type(b)  # revealed: Array[Batch, Height, Width]
    reveal_type(del_batch_axis(b))  # revealed: Array[Height, Width]
    reveal_type(add_batch_channels(a))  # revealed: Array[Batch, Height, Width, Channels]
    reveal_type(del_channels_axis(c))  # revealed: Array[Height, Width]
    reveal_type(prefix_tuple(1, (True, "a")))  # revealed: tuple[Literal[1], Literal[True], Literal["a"]]
```

## Unpacking Tuple Types

### Unbounded and nested tuple types

`Unpack[...]` can express an unknown middle section and heterogeneous positional arguments in the
same locations as starred tuple unpacking.

```py
from typing import Any, TypeVarTuple, Unpack

Items = TypeVarTuple("Items")

def accept_packet(x: tuple[bytes, Unpack[tuple[Any, ...]], int]) -> None: ...
def payload(x: tuple[bytes, Unpack[Items], int]) -> tuple[Unpack[Items]]:
    raise NotImplementedError

def parse_log(*args: Unpack[tuple[bool, Unpack[tuple[str, ...]], bytes]]) -> None: ...
def remove_checksum(*args: Unpack[tuple[Unpack[Items], bytes]]) -> tuple[Unpack[Items]]:
    raise NotImplementedError

def f(
    multi: tuple[bytes, str, bool, int],
    empty: tuple[bytes, int],
    truncated: tuple[bytes],
    dynamic: tuple[bytes, Unpack[tuple[Any, ...]], int],
) -> None:
    accept_packet(multi)
    accept_packet(empty)
    accept_packet(truncated)  # error: [invalid-argument-type]
    reveal_type(payload(dynamic))  # revealed: tuple[Any, ...]
    parse_log(True, "phase", "status", b"ok")
    parse_log(True, b"ok")
    parse_log(True, 1, b"bad")  # error: [invalid-argument-type]
    reveal_type(remove_checksum(1, "record", b"sum"))  # revealed: tuple[Literal[1], Literal["record"]]
```

## Type Aliases

### Legacy generic aliases

```py
from typing import TypeVar, TypeVarTuple, Unpack

T = TypeVar("T")
Ts = TypeVarTuple("Ts")
U = TypeVar("U")

Alias = tuple[T, Unpack[Ts], U]
Prefix = tuple[int, Unpack[Ts]]
Suffix = tuple[Unpack[Ts], str]

def f(
    alias: Alias[int, bool, str],
    long_alias: Alias[int, bool, bytes, str],
    short_alias: Alias[int, str],
    prefix: Prefix[bool, str],
    suffix: Suffix[int, bool],
) -> None:
    reveal_type(alias)  # revealed: tuple[int, bool, str]
    reveal_type(long_alias)  # revealed: tuple[int, bool, bytes, str]
    reveal_type(short_alias)  # revealed: tuple[int, str]
    reveal_type(prefix)  # revealed: tuple[int, bool, str]
    reveal_type(suffix)  # revealed: tuple[int, bool, str]
```

### Unpacked tuple type arguments

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

Alias = tuple[int, Unpack[Ts]]

def f(x: Alias[Unpack[tuple[str, bool]]], y: Alias[Unpack[tuple[str, ...]]]) -> None:
    reveal_type(x)  # revealed: tuple[int, str, bool]
    reveal_type(y)  # revealed: tuple[int, *tuple[str, ...]]
```

### Unspecified alias type arguments

A bare variadic alias substitutes an unknown-length tuple of `Any` when the legacy unpack spelling
is used.

```py
from typing import Any, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

Headered = tuple[bytes, Unpack[Ts]]

def f(raw: Headered, explicit: Headered[Unpack[tuple[Any, ...]]]) -> None:
    reveal_type(raw)  # revealed: tuple[bytes, *tuple[Any, ...]]
    reveal_type(explicit)  # revealed: tuple[bytes, *tuple[Any, ...]]
```

### Variadic substitutions

Legacy aliases can forward an unpacked type variable tuple and split an unpacked unbounded tuple to
satisfy a fixed trailing type argument.

```py
from typing import TypeVar, TypeVarTuple, Unpack

Start = TypeVar("Start")
End = TypeVar("End")
Ts = TypeVarTuple("Ts")

Payload = tuple[bytes, Unpack[Ts]]
CountedPayload = Payload[int, Unpack[Ts]]
Started = tuple[Start, Unpack[Ts]]
Terminated = tuple[Unpack[Ts], End]

def f(
    forwarded: CountedPayload[str, bool],
    leading_split: Started[Unpack[tuple[str, ...]]],
    split: Terminated[Unpack[tuple[str, ...]]],
    retained: Terminated[Unpack[tuple[str, ...]], bytes],
    combined: Terminated[int, Unpack[tuple[str, ...]]],
) -> None:
    reveal_type(forwarded)  # revealed: tuple[bytes, int, str, bool]
    reveal_type(leading_split)  # revealed: tuple[str, *tuple[str, ...]]
    reveal_type(split)  # revealed: tuple[*tuple[str, ...], str]
    reveal_type(retained)  # revealed: tuple[*tuple[str, ...], bytes]
    reveal_type(combined)  # revealed: tuple[int, *tuple[str, ...], str]
```

## Invalid Tuple Forms

### Only one variadic unpack

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

def f(
    ok1: tuple[int, Unpack[Ts]],
    ok2: tuple[int, Unpack[Ts], str],
    bad1: tuple[Unpack[Ts], Unpack[tuple[str, ...]]],  # error: [invalid-type-form]
    bad2: tuple[Unpack[tuple[str, ...]], Unpack[Ts]],  # error: [invalid-type-form]
) -> None: ...
```
