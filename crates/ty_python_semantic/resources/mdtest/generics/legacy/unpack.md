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

# TODO: revealed: tuple[()]
reveal_type(Array[()]().value)  # revealed: @Todo(ParamSpecs and TypeVarTuples)
# TODO: revealed: tuple[int, str]
reveal_type(Array[int, str]().value)  # revealed: @Todo(ParamSpecs and TypeVarTuples)
# TODO: revealed: tuple[int, str]
reveal_type(Array[Unpack[tuple[int, str]]]().value)  # revealed: @Todo(ParamSpecs and TypeVarTuples)
```

## Variadic parameter inference

An unpacked type variable tuple used for `*args` preserves the number and types of positional
arguments.

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

def collect(*args: Unpack[Ts]) -> tuple[Unpack[Ts]]:
    # TODO: revealed: tuple[*Ts@collect]
    reveal_type(args)  # revealed: tuple[@Todo(`Unpack[]` special form), ...]
    raise NotImplementedError

# TODO: revealed: tuple[()]
reveal_type(collect())  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[Literal[1], Literal["a"]]
reveal_type(collect(1, "a"))  # revealed: tuple[@Todo(TypeVarTuple), ...]
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
# TODO: error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `(Literal[1], /) -> str`, found `def format_value(value: int, label: str, /) -> str`"
reveal_type(invoke(format_value, 1))  # revealed: str
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
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(fixed)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, *tuple[str, ...]]
    reveal_type(unbounded)  # revealed: tuple[@Todo(TypeVarTuple), ...]
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

# TODO: revealed: tuple[int, str]
reveal_type(WithDefault().value)  # revealed: @Todo(ParamSpecs and TypeVarTuples)
# TODO: revealed: tuple[bool, bytes]
reveal_type(WithDefault[bool, bytes]().value)  # revealed: @Todo(ParamSpecs and TypeVarTuples)
```

## Validation

`Unpack` requires a tuple operand, and a tuple specialization can contain only one variadic unpack.

```py
from typing import Generic, TypeVar, TypeVarTuple, Unpack

U = TypeVar("U")
Ts = TypeVarTuple("Ts")

class Pair(Generic[Unpack[Ts], U]): ...

# TODO: error: [invalid-type-form] "`Unpack` can only unpack a tuple type or `TypeVarTuple`"
class InvalidGeneric(Generic[U, Unpack[int]]): ...

def invalid(
    # TODO: error: [invalid-type-form] "`Unpack` can only unpack a tuple type or `TypeVarTuple`"
    non_tuple: Pair[Unpack[int], str],
    # error: [invalid-type-form] "Multiple unpacked variadic tuples are not allowed in a `tuple` specialization"
    multiple: tuple[Unpack[Ts], Unpack[tuple[str, ...]]],
) -> None:
    # TODO: revealed: Pair[*tuple[Unknown, ...], str]
    reveal_type(non_tuple)  # revealed: @Todo(specialized non-generic class)

# TODO: error: [invalid-type-form] "`Unpack` can only unpack a tuple type or `TypeVarTuple`"
def invalid_vararg(*args: Unpack[int]) -> None:
    # TODO: revealed: tuple[Unknown, ...]
    reveal_type(args)  # revealed: tuple[@Todo(`Unpack[]` special form), ...]

# TODO: error: [invalid-type-form] "`Unpack` can only unpack a tuple type or `TypeVarTuple`"
def invalid_stringified_vararg(*args: "Unpack[int]") -> None:
    # TODO: revealed: tuple[Unknown, ...]
    reveal_type(args)  # revealed: tuple[@Todo(`Unpack[]` special form), ...]

# TODO: error: [invalid-type-form] "`Unpack` cannot be nested"
def nested(*args: Unpack[Unpack[tuple[int, ...]]]) -> None: ...

# TODO: error: [invalid-type-form] "Bare TypeVarTuple `Ts` is not valid in this context in a parameter annotation"
def nested_bare_typevartuple(*args: Unpack[tuple[Ts]]) -> None: ...
```
