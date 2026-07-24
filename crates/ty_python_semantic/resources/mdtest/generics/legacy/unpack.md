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

def collect(*args: Unpack[Ts]) -> tuple[Unpack[Ts]]:
    reveal_type(args)  # revealed: tuple[*Ts@collect]
    raise NotImplementedError

# TODO: Infer the `TypeVarTuple` from arguments matched to the variadic parameter.
reveal_type(collect())  # revealed: tuple[Unknown, ...]
reveal_type(collect(1, "a"))  # revealed: tuple[Unknown, ...]
```

## Callable parameters

`Unpack` expands a type variable tuple into a callable's positional parameter list. The same tuple
can describe the arguments forwarded to that callable.

```py
from typing import Callable, TypeVar, TypeVarTuple, Unpack

R = TypeVar("R")
Ts = TypeVarTuple("Ts")

def invoke(
    callback: Callable[[Unpack[Ts]], R],
    *args: Unpack[Ts],
) -> R:
    raise NotImplementedError

def format_value(value: int, label: str, /) -> str:
    return f"{label}: {value}"

reveal_type(invoke(format_value, 1, "value"))  # revealed: str
# TODO: Validate arguments matched to the variadic parameter against the `TypeVarTuple` inferred
# from the callback.
reveal_type(invoke(format_value, 1))  # revealed: str
```

A callable that forwards a `ParamSpec` can also be passed with its arguments to a callable whose
parameters are described by a `TypeVarTuple`:

```py
from typing import Awaitable, Callable, ParamSpec, TypeVarTuple, Unpack

P = ParamSpec("P")
Ts = TypeVarTuple("Ts")

def start_soon(
    async_fn: Callable[[Unpack[Ts]], Awaitable[object]],
    *args: Unpack[Ts],
) -> None: ...
async def forward(
    async_fn: Callable[P, Awaitable[object]],
    *args: P.args,
    **kwargs: P.kwargs,
) -> None: ...
async def one_arg(value: int) -> None: ...

start_soon(forward, one_arg, 1)
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
# TODO: error: [invalid-argument-type] "Argument to function `accept` is incorrect: Expected `tuple[bool, *tuple[str, ...], bytes]`"
accept(True, 1, b"bad")
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
