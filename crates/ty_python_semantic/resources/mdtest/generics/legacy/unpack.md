# Legacy `typing.Unpack`

```toml
[environment]
python-version = "3.11"
```

`Unpack[Ts]` is the legacy spelling of `*Ts`. The shared semantics of type variable tuples are
covered in `../pep695/typevartuple.md`; this file checks the distinct syntax paths used by `Unpack`.

## Generic specialization

`Unpack` can introduce a type variable tuple in a legacy generic declaration. An unpacked fixed
tuple can also provide multiple type arguments when specializing the generic.

```py
from typing import Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

class Array(Generic[Unpack[Ts]]):
    value: tuple[Unpack[Ts]]

reveal_type(Array[()]().value)  # revealed: tuple[()]
reveal_type(Array[int, str]().value)  # revealed: tuple[int, str]
reveal_type(Array[Unpack[tuple[int, str]]]().value)  # revealed: tuple[int, str]
```

## Variadic parameter inference

An unpacked type variable tuple used for `*args` preserves the number and types of positional
arguments.

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")
Us = TypeVarTuple("Us")

def collect(*args: Unpack[Ts]) -> tuple[Unpack[Ts]]:
    reveal_type(args)  # revealed: tuple[*Ts@collect]
    raise NotImplementedError

def forward(*args: Unpack[Us]) -> tuple[Unpack[Us]]:
    reveal_type(collect(*args))  # revealed: tuple[*Us@forward]
    return collect(*args)

reveal_type(collect())  # revealed: tuple[()]

def check(i: int, s: str, fixed: tuple[int, str]) -> None:
    reveal_type(collect(i, s))  # revealed: tuple[int, str]
    reveal_type(collect(*fixed))  # revealed: tuple[int, str]
```

## Callable parameters

`Unpack` expands a type variable tuple into a callable's positional parameter list. The same tuple
can describe the arguments forwarded to that callable.

```py
from typing import Callable, TypeVar, TypeVarTuple, Unpack, overload

R = TypeVar("R")
Ts = TypeVarTuple("Ts")

def invoke(
    callback: Callable[[Unpack[Ts]], R],
    *args: Unpack[Ts],
) -> R:
    raise NotImplementedError

def invoke_tuple(
    callback: Callable[[Unpack[Ts]], tuple[Unpack[Ts]]],
    *args: Unpack[Ts],
) -> tuple[Unpack[Ts]]:
    raise NotImplementedError

def invoke_pack(
    callback: Callable[[Unpack[Ts]], object],
    *args: Unpack[Ts],
) -> tuple[Unpack[Ts]]:
    raise NotImplementedError

def no_arguments() -> str:
    return "empty"

def format_value(value: int, label: str, /) -> str:
    return f"{label}: {value}"

reveal_type(invoke(format_value, 1, "value"))  # revealed: str
# error: [invalid-argument-type]
reveal_type(invoke(format_value, 1))  # revealed: str

reveal_type(invoke_pack(no_arguments))  # revealed: tuple[()]

def check_pack(value: int, label: str) -> None:
    reveal_type(invoke_pack(format_value, value, label))  # revealed: tuple[int, str]

empty = invoke_pack(
    format_value,  # error: [invalid-argument-type]
    value=1,  # error: [unknown-argument]
    label="value",  # error: [unknown-argument]
)
reveal_type(empty)  # revealed: tuple[()]

partial = invoke_pack(
    format_value,  # error: [invalid-argument-type]
    1,
    label="value",  # error: [unknown-argument]
)
reveal_type(partial)  # revealed: tuple[Literal[1]]

@overload
def overloaded_value(value: int) -> str: ...
@overload
def overloaded_value(value: str) -> str: ...
def overloaded_value(value: int | str) -> str:
    return str(value)

@overload
def returns_string_tuple(value: int, /) -> tuple[str]: ...
@overload
def returns_string_tuple(value: str, /) -> tuple[str]: ...
def returns_string_tuple(value: int | str, /) -> tuple[str]:
    return (str(value),)

def returns_string_tuple_once(value: object, /) -> tuple[str]:
    return (str(value),)

def accepts_str_once(value: str, /) -> object:
    return value

def check_tuple_return(value: int) -> None:
    result = invoke_tuple(
        returns_string_tuple,  # error: [invalid-argument-type]
        value,
    )
    reveal_type(result)  # revealed: tuple[int]

    single_result = invoke_tuple(
        returns_string_tuple_once,  # error: [invalid-argument-type]
        value,
    )
    reveal_type(single_result)  # revealed: tuple[int]

    parameter_result = invoke_pack(
        accepts_str_once,  # error: [invalid-argument-type]
        value,
    )
    reveal_type(parameter_result)  # revealed: tuple[int]

reveal_type(invoke(overloaded_value, 1))  # revealed: str
reveal_type(invoke(overloaded_value, "value"))  # revealed: str
# error: [invalid-argument-type]
invoke(overloaded_value, 1.0)

overloaded_empty = invoke_pack(
    overloaded_value,  # error: [invalid-argument-type]
    value=1,  # error: [unknown-argument]
)
reveal_type(overloaded_empty)  # revealed: tuple[()]
```

## Ecosystem generic forwarding callbacks

A legacy `Unpack` forwarding callback must be specialized for the arguments already matched by the
outer variadic parameter.

```py
from collections.abc import Awaitable, Callable
from typing import TypeVar, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")
Us = TypeVarTuple("Us")
R = TypeVar("R")

def schedule(callback: Callable[[Unpack[Ts]], object], *args: Unpack[Ts]) -> None: ...
def run_sync(callback: Callable[[Unpack[Us]], R], *args: Unpack[Us]) -> Awaitable[R]:
    raise NotImplementedError

def target(value: int) -> int:
    return value

schedule(run_sync, target, 1)
```

## Ecosystem platform-unknown forwarding callbacks

A value defined only on another platform is unreachable on the current platform. Forwarding it must
not erase the matched argument count or produce a legacy `Unpack` callback error.

```py
import os
from collections.abc import Awaitable, Callable
from typing import TypeVar, TypeVarTuple, Unpack
from ty_extensions import Unknown

Ts = TypeVarTuple("Ts")
Us = TypeVarTuple("Us")
R = TypeVar("R")

if os.name == "nt":
    def make_platform_handle() -> int:
        return 1

class Nursery:
    def start_soon(
        self,
        callback: Callable[[Unpack[Ts]], Awaitable[object]],
        *args: Unpack[Ts],
        name: object = None,
    ) -> None: ...

async def run_sync(
    callback: Callable[[Unpack[Us]], R],
    *args: Unpack[Us],
    name: str | None = None,
) -> R:
    raise NotImplementedError

async def signal(value: int) -> None: ...
async def signal_pair(first: int, second: int) -> None: ...
def synchronous(value: int) -> int:
    return value

def synchronous_pair(first: int, second: int) -> int:
    return first + second

def reveal_platform_handle() -> None:
    reveal_type(make_platform_handle())  # revealed: Never

def check(nursery: Nursery) -> None:
    handle = make_platform_handle()
    nursery.start_soon(signal, handle)
    nursery.start_soon(run_sync, synchronous, handle)
    nursery.start_soon(signal_pair, handle, handle)
    nursery.start_soon(run_sync, synchronous_pair, handle, handle)
    nursery.start_soon(signal, "invalid")  # error: [invalid-argument-type]

def check_ordinary_unknown(nursery: Nursery, value: Unknown) -> None:
    nursery.start_soon(signal, value)
    nursery.start_soon(signal_pair, value, value)
    nursery.start_soon(signal, value, value)  # error: [invalid-argument-type]

def check_open_unknown(nursery: Nursery, values: tuple[Unknown, ...]) -> None:
    nursery.start_soon(signal_pair, *values)  # error: [invalid-argument-type]
```

## Type aliases

A legacy alias can use `Unpack[Ts]` and accept either individual types or an unpacked tuple type.

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

Alias = tuple[int, Unpack[Ts]]

def f(
    fixed: Alias[str, bool],
    unbounded: Alias[Unpack[tuple[str, ...]]],
) -> None:
    reveal_type(fixed)  # revealed: tuple[int, str, bool]
    reveal_type(unbounded)  # revealed: tuple[int, *tuple[str, ...]]
```

## Unsupported union unpacking

Unpacking a type variable tuple into `Union` is currently not supported. Both the rejected union and
runtime element access recover to `object`.

```py
from typing import TypeVarTuple, Union, Unpack

Ts = TypeVarTuple("Ts")

# TODO: shouldn't error
# error: [invalid-type-form]
def reject_union(value: Union[Unpack[Ts]]) -> None:
    # TODO: should reveal `Union[*Ts]` representation
    reveal_type(value)  # revealed: object

def element_types(values: tuple[Unpack[Ts]]) -> None:
    # TODO: should reveal `Union[*Ts]` representation
    reveal_type(values[0])  # revealed: object

    for value in values:
        # TODO: should reveal `Union[*Ts]` representation
        reveal_type(value)  # revealed: object
```

## Concrete and nested tuple unpacking

`Unpack` can expand a concrete tuple annotation for `*args`, including a nested unbounded tuple.

```py
from typing import Unpack

def accept(
    *args: Unpack[tuple[bool, Unpack[tuple[str, ...]], bytes]],
) -> None: ...

accept(True, "phase", "status", b"ok")
accept(True, b"ok")
# error: [invalid-argument-type] "Argument to function `accept` is incorrect: Expected `tuple[bool, *tuple[str, ...], bytes]`"
accept(True, 1, b"bad")

def concrete(
    *args: Unpack[tuple[str, Unpack[tuple[str, ...]], dict[str, str]]],
) -> None: ...
def check_open_splat(values: list[str], bad: list[int], env: dict[str, str]) -> None:
    concrete(*values, env)
    concrete(
        *bad,  # error: [invalid-argument-type]
        env,
    )
```

## Defaults

A type variable tuple default can use `Unpack`, and an explicit specialization overrides it.

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts", default=Unpack[tuple[int, str]])

class WithDefault(Generic[Unpack[Ts]]):
    value: tuple[Unpack[Ts]]

reveal_type(WithDefault().value)  # revealed: tuple[int, str]
reveal_type(WithDefault[bool, bytes]().value)  # revealed: tuple[bool, bytes]
```

## Validation

`Unpack` requires a tuple operand, and a tuple specialization can contain only one variadic unpack.

```py
from typing import Generic, TypeVar, TypeVarTuple, Unpack

U = TypeVar("U")
Ts = TypeVarTuple("Ts")
Xs = TypeVarTuple("Xs")
Ys = TypeVarTuple("Ys")

class Pair(Generic[Unpack[Ts], U]): ...

# error: [invalid-generic-class] "Only one `TypeVarTuple` parameter is allowed in a `Generic` subscription"
class MultipleUnpack(Generic[Unpack[Xs], Unpack[Ys]]): ...

# error: [invalid-generic-class] "Only one `TypeVarTuple` parameter is allowed in a `Generic` subscription"
class StarThenUnpack(Generic[*Xs, Unpack[Ys]]): ...

# error: [invalid-generic-class] "Only one `TypeVarTuple` parameter is allowed in a `Generic` subscription"
class UnpackThenStar(Generic[Unpack[Xs], *Ys]): ...

def invalid(
    # error: [invalid-type-form] "`Unpack` can only unpack a tuple type or `TypeVarTuple`"
    non_tuple: Pair[Unpack[int], str],
    # error: [invalid-type-form] "Multiple unpacked variadic tuples are not allowed in a `tuple` specialization"
    multiple: tuple[Unpack[Ts], Unpack[tuple[str, ...]]],
) -> None:
    reveal_type(non_tuple)  # revealed: Pair[*tuple[Unknown, ...], str]

# error: [invalid-type-form] "`Unpack` can only unpack a tuple type or `TypeVarTuple`"
def invalid_vararg(*args: Unpack[int]) -> None:
    reveal_type(args)  # revealed: tuple[Unknown, ...]

# error: [invalid-type-form] "`Unpack` can only unpack a tuple type or `TypeVarTuple`"
def invalid_stringified_vararg(*args: "Unpack[int]") -> None:
    reveal_type(args)  # revealed: tuple[Unknown, ...]

# error: [invalid-type-form] "`Unpack` cannot be nested"
def nested(*args: Unpack[Unpack[tuple[int, ...]]]) -> None: ...

# error: [invalid-type-form] "Bare TypeVarTuple `Ts` is not valid in this context in a parameter annotation"
def nested_bare_typevartuple(*args: Unpack[tuple[Ts]]) -> None: ...
```
