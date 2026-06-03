# PEP 695 `TypeVarTuple`

```toml
[environment]
python-version = "3.12"
```

## Definition

A PEP 695 type variable tuple is introduced with a starred type parameter.

```py
def f[*Ts]() -> None:
    reveal_type(Ts)  # revealed: TypeVarTuple
```

## Variance inference

PEP 695 type variable tuples infer variance from how the class uses them.

```py
class CovariantArray[*Ts]:
    def get(self) -> tuple[*Ts]:
        raise NotImplementedError

covariant_ok: CovariantArray[object] = CovariantArray[int]()
covariant_error: CovariantArray[int] = CovariantArray[object]()  # error: [invalid-assignment]

class ContravariantArray[*Ts]:
    def set(self, value: tuple[*Ts]) -> None:
        raise NotImplementedError

contravariant_ok: ContravariantArray[int] = ContravariantArray[object]()
contravariant_error: ContravariantArray[object] = ContravariantArray[int]()  # error: [invalid-assignment]

class InvariantArray[*Ts]:
    values: tuple[*Ts]

invariant_out: InvariantArray[object] = InvariantArray[int]()  # error: [invalid-assignment]
invariant_in: InvariantArray[int] = InvariantArray[object]()  # error: [invalid-assignment]
```

## Generic Classes

### Explicit specialization

```py
class Array[*Ts]:
    def shape(self) -> tuple[*Ts]:
        raise NotImplementedError

class Sandwich[T, *Ts, U]:
    def parts(self) -> tuple[T, *Ts, U]:
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
class Array[*Ts]:
    def __init__(self, shape: tuple[*Ts]) -> None:
        self.shape = shape

def f(i: int, s: str) -> None:
    reveal_type(Array((i, s)))  # revealed: Array[int, str]
    reveal_type(Array(()))  # revealed: Array[()]
```

### Default type arguments

A defaulted type variable tuple supplies its unpacked tuple when the generic class is not explicitly
specialized. Explicit type arguments override the default.

```toml
[environment]
python-version = "3.13"
```

```py
class Array[*Ts = *tuple[int, str]]:
    def shape(self) -> tuple[*Ts]:
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

An unsubscripted variadic generic uses an unknown-length tuple of `Any` arguments, allowing typed
and dynamic uses of the class to interoperate.

```py
from typing import Any

class Shelf[*Ts]:
    def contents(self) -> tuple[*Ts]:
        raise NotImplementedError

def needs_two_items(x: Shelf[int, str]) -> None:
    raise NotImplementedError

def needs_dynamic_items(x: Shelf) -> None:
    raise NotImplementedError

def f(
    dynamic: Shelf,
    explicitly_dynamic: Shelf[*tuple[Any, ...]],
    fixed: Shelf[int, str],
) -> None:
    reveal_type(dynamic)  # revealed: Shelf[*tuple[Any, ...]]
    reveal_type(dynamic.contents())  # revealed: tuple[Any, ...]
    reveal_type(explicitly_dynamic)  # revealed: Shelf[*tuple[Any, ...]]
    reveal_type(explicitly_dynamic.contents())  # revealed: tuple[Any, ...]
    needs_two_items(dynamic)
    needs_dynamic_items(fixed)
```

### Assignment checks

```py
class Array[*Ts]:
    values: tuple[*Ts]

def takes_int_str(x: Array[int, str]) -> None: ...
def takes_int_str_tuple(x: tuple[int, str]) -> None: ...
def f(x: Array[int, str], y: Array[str, int], xs: tuple[int, str], ys: tuple[str, int]) -> None:
    takes_int_str(x)
    takes_int_str(y)  # error: [invalid-argument-type]
    takes_int_str_tuple(xs)
    takes_int_str_tuple(ys)  # error: [invalid-argument-type]
```

## Functions

### Tuple arguments and returns

```py
def echo[*Ts](x: tuple[*Ts]) -> tuple[*Ts]:
    return x

def with_prefix[T, *Ts](x: T, y: tuple[*Ts]) -> tuple[T, *Ts]:
    raise NotImplementedError

def with_suffix[*Ts, U](x: tuple[*Ts], y: U) -> tuple[*Ts, U]:
    raise NotImplementedError

def with_both[T, *Ts, U](x: tuple[T, *Ts, U]) -> tuple[*Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(echo((i, s)))  # revealed: tuple[int, str]
    reveal_type(echo(()))  # revealed: tuple[()]
    reveal_type(with_prefix(i, (s, b)))  # revealed: tuple[int, str, bool]
    reveal_type(with_suffix((i, s), b))  # revealed: tuple[int, str, bool]
    reveal_type(with_both((i, s, b)))  # revealed: tuple[str]
```

### Starred variadic parameters

When a `TypeVarTuple` appears in `*args`, the argument tuple keeps the exact element types from the
call site.

```py
def args_to_tuple[*Ts](*args: *Ts) -> tuple[*Ts]:
    reveal_type(args)  # revealed: tuple[*Ts@args_to_tuple]
    raise NotImplementedError

def first_and_rest[T, *Ts](first: T, *rest: *Ts) -> tuple[T, *Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(args_to_tuple(i, s))  # revealed: tuple[int, str]
    reveal_type(args_to_tuple())  # revealed: tuple[()]
    reveal_type(first_and_rest(i, s, b))  # revealed: tuple[int, str, bool]
```

### Callable parameters

`Callable` accepts unpacked `TypeVarTuple`s in its positional parameter list.

```py
from typing import Callable

def f[*Ts](callback: Callable[[int, *Ts], tuple[*Ts]]) -> None:
    reveal_type(callback)  # revealed: (int, /, *Ts@f) -> tuple[*Ts@f]

def invoke[*Ts, R](callback: Callable[[*Ts], R], args: tuple[*Ts]) -> R:
    raise NotImplementedError

def encode(label: str, count: int) -> bytes:
    raise NotImplementedError

def calls() -> None:
    reveal_type(invoke(encode, ("item", 2)))  # revealed: bytes
    invoke(encode, (2, "item"))  # error: [invalid-argument-type]
```

### Length-sensitive inference

```py
def same_shape[*Ts](x: tuple[*Ts], y: tuple[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(same_shape((i, s), (b, i)))  # revealed: tuple[int, str | int]
    same_shape((i,), (s, b))  # error: [invalid-argument-type]
```

## Type concatenation

A type variable tuple can be combined with fixed leading or trailing types.

```py
class Array[*Shape]: ...
class Batch: ...
class Channels: ...
class Height: ...
class Width: ...

def add_batch_axis[*Shape](x: Array[*Shape]) -> Array[Batch, *Shape]:
    raise NotImplementedError

def del_batch_axis[*Shape](x: Array[Batch, *Shape]) -> Array[*Shape]:
    raise NotImplementedError

def add_batch_channels[*Shape](x: Array[*Shape]) -> Array[Batch, *Shape, Channels]:
    raise NotImplementedError

def del_channels_axis[*Shape](x: Array[*Shape, Channels]) -> Array[*Shape]:
    raise NotImplementedError

def prefix_tuple[T, *Shape](x: T, y: tuple[*Shape]) -> tuple[T, *Shape]:
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

### Unbounded tuple types

An unpacked unbounded tuple can describe an unknown middle section while retaining fixed endpoints,
and it can be passed into a function that solves a type variable tuple.

```py
from typing import Any

def accept_packet(x: tuple[bytes, *tuple[Any, ...], int]) -> None: ...
def payload[*Items](x: tuple[bytes, *Items, int]) -> tuple[*Items]:
    raise NotImplementedError

def f(
    multi: tuple[bytes, str, bool, int],
    empty: tuple[bytes, int],
    truncated: tuple[bytes],
    dynamic: tuple[bytes, *tuple[Any, ...], int],
) -> None:
    accept_packet(multi)
    accept_packet(empty)
    accept_packet(truncated)  # error: [invalid-argument-type]
    reveal_type(payload(dynamic))  # revealed: tuple[Any, ...]
```

### Nested unpacking in variadic parameters

A tuple containing an unpacked tuple can precisely describe heterogeneous positional arguments,
including a variable-length middle portion or a type-variable prefix.

```py
def parse_log(*args: *tuple[bool, *tuple[str, ...], bytes]) -> None: ...
def remove_checksum[*Prefix](*args: *tuple[*Prefix, bytes]) -> tuple[*Prefix]:
    raise NotImplementedError

def f() -> None:
    parse_log(True, "phase", "status", b"ok")
    parse_log(True, b"ok")
    parse_log(True, 1, b"bad")  # error: [invalid-argument-type]
    reveal_type(remove_checksum(1, "record", b"sum"))  # revealed: tuple[Literal[1], Literal["record"]]
```

## Type Aliases

### Variadic aliases

```py
type Identity[*Ts] = tuple[*Ts]
type Prefix[*Ts] = tuple[int, *Ts]
type Suffix[*Ts] = tuple[*Ts, str]
type Sandwich[T, *Ts, U] = tuple[T, *Ts, U]

def f(
    identity: Identity[int, str],
    empty: Identity[()],
    prefix: Prefix[bool, str],
    suffix: Suffix[int, bool],
    sandwich1: Sandwich[int, bool, str],
    long_sandwich: Sandwich[int, bool, bytes, str],
    sandwich2: Sandwich[int, str],
) -> None:
    reveal_type(identity)  # revealed: tuple[int, str]
    reveal_type(empty)  # revealed: tuple[()]
    reveal_type(prefix)  # revealed: tuple[int, bool, str]
    reveal_type(suffix)  # revealed: tuple[int, bool, str]
    reveal_type(sandwich1)  # revealed: tuple[int, bool, str]
    reveal_type(long_sandwich)  # revealed: tuple[int, bool, bytes, str]
    reveal_type(sandwich2)  # revealed: tuple[int, str]
```

### Unpacked tuple type arguments

```py
type Prefix[*Ts] = tuple[int, *Ts]

def f(x: Prefix[*tuple[str, bool]], y: Prefix[*tuple[str, ...]]) -> None:
    reveal_type(x)  # revealed: tuple[int, str, bool]
    reveal_type(y)  # revealed: tuple[int, *tuple[str, ...]]
```

### Unspecified alias type arguments

A bare variadic alias substitutes an unknown-length tuple of `Any`, just like a bare variadic
generic class.

```py
from typing import Any

type Headered[*Fields] = tuple[bytes, *Fields]

def f(raw: Headered, explicit: Headered[*tuple[Any, ...]]) -> None:
    reveal_type(raw)  # revealed: tuple[bytes, *tuple[Any, ...]]
    reveal_type(explicit)  # revealed: tuple[bytes, *tuple[Any, ...]]
```

### Variadic substitutions

A variadic alias can forward its remaining arguments to another variadic alias. When an unbounded
tuple is supplied to an alias with a fixed trailing argument, enough elements are used for that
fixed argument and the remaining unbounded portion stays variadic.

```py
type Payload[*Items] = tuple[bytes, *Items]
type CountedPayload[*Items] = Payload[int, *Items]
type Started[Start, *Items] = tuple[Start, *Items]
type Terminated[*Items, End] = tuple[*Items, End]

def f(
    forwarded: CountedPayload[str, bool],
    leading_split: Started[*tuple[str, ...]],
    split: Terminated[*tuple[str, ...]],
    retained: Terminated[*tuple[str, ...], bytes],
    combined: Terminated[int, *tuple[str, ...]],
) -> None:
    reveal_type(forwarded)  # revealed: tuple[bytes, int, str, bool]
    reveal_type(leading_split)  # revealed: tuple[str, *tuple[str, ...]]
    reveal_type(split)  # revealed: tuple[*tuple[str, ...], str]
    reveal_type(retained)  # revealed: tuple[*tuple[str, ...], bytes]
    reveal_type(combined)  # revealed: tuple[int, *tuple[str, ...], str]
```

### A type variable tuple cannot provide a fixed alias argument

An unbounded concrete tuple can be divided to provide a required fixed type argument, but an unknown
type variable tuple cannot be inspected to take its first element.

```py
type Headed[Head, *Tail] = tuple[Head, *Tail]

# error: [invalid-type-form]
type InvalidForward[*Items] = Headed[*Items]
```

## Accessing Individual Types

Operations that need to rearrange individual members of a type variable tuple can expose overloads
for each supported tuple length.

```py
from typing import overload

class Row[*Cells]:
    def cells(self) -> tuple[*Cells]:
        raise NotImplementedError

    @overload
    def rotate_left[A, B](self: "Row[A, B]") -> "Row[B, A]": ...
    @overload
    def rotate_left[A, B, C](self: "Row[A, B, C]") -> "Row[B, C, A]": ...
    def rotate_left(self) -> "Row":
        raise NotImplementedError

def f(pair: Row[int, str], triple: Row[int, str, bytes]) -> None:
    reveal_type(pair.rotate_left())  # revealed: Row[str, int]
    reveal_type(triple.rotate_left())  # revealed: Row[str, bytes, int]
```

## Invalid Forms

### Multiple Type Variable Tuples not allowed

Only one type variable tuple can appear in a type parameter list.

```py
# error: [invalid-type-form]
class Array[*Ts1, *Ts2]: ...
```

### Must always be unpacked

```py
def invalid[*Ts](x: Ts) -> None: ...  # error: [invalid-type-form]
def invalid_args[*Ts](*args: Ts) -> None: ...  # error: [invalid-type-form]
def valid[*Ts](x: tuple[*Ts]) -> tuple[*Ts]:
    return x
```

### Only one variadic unpack

```py
def f[*Ts](
    ok1: tuple[int, *Ts],
    ok2: tuple[int, *Ts, str],
    bad1: tuple[*Ts, *tuple[str, ...]],  # error: [invalid-type-form]
    bad2: tuple[*tuple[str, ...], *Ts],  # error: [invalid-type-form]
) -> None: ...
```
