# Selecting rules for mismatched type names

## Existing configuration

The deprecated `mismatched-type-name` setting applies to all eight rules. A setting for an
individual kind of type takes precedence within the same table.

```toml
[rules]
mismatched-type-name = "ignore"
mismatched-typevar-name = "error"
```

```py
from enum import IntFlag
from typing import NamedTuple, NewType, ParamSpec, TypedDict, TypeVar
from typing_extensions import TypeAliasType, TypeVarTuple

Flags = IntFlag("Options", "READ WRITE")  # ok
Point = NamedTuple("Vector", [("x", int)])  # ok
UserId = NewType("Id", int)  # ok
P = ParamSpec("Q")  # ok
Number = TypeAliasType("Num", int)  # ok
Movie = TypedDict("Film", {"title": str})  # ok
Ts = TypeVarTuple("Us")  # ok

# snapshot: mismatched-typevar-name
T = TypeVar("U")
```

```snapshot
error[mismatched-typevar-name]: The name passed to `TypeVar` must match the variable it is assigned to
  --> src/mdtest_snippet.py:14:13
   |
14 | T = TypeVar("U")
   |             ^^^ Expected "T", got "U"
```

## Existing ignore comments

An ignore using the old name can suppress any of the eight diagnostics. A rule-specific ignore only
suppresses mismatches for its own kind of type.

```py
from collections import namedtuple
from typing import NewType, TypeVar

Point = namedtuple("Vector", "x y")  # ty: ignore[mismatched-type-name]
T = TypeVar("U")  # type: ignore[ty:mismatched-type-name]

# error: [mismatched-newtype-name]
# error: [unused-ignore-comment]
UserId = NewType("Id", int)  # ty: ignore[mismatched-typevar-name]
```

An unused ignore reports the name that appeared in the comment.

```py
# error: [unused-ignore-comment] "Unused `ty: ignore` directive: 'mismatched-type-name'"
missing  # ty: ignore[mismatched-type-name, unresolved-reference]
```

## Names specified by type aliases

An alias for a single string literal provides the exact name to compare with the assigned variable.
For type parameters and aliases created with `TypeAliasType`, ty also infers a literal type for
`__name__`.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal, NewType, ParamSpec, TypeAliasType, TypedDict, TypeVar, TypeVarTuple

type TypeVarName = Literal["T"]
type ParamSpecName = Literal["P"]
type TypeVarTupleName = Literal["Ts"]
type NewTypeName = Literal["UserId"]
type TypeAliasName = Literal["Number"]
type TypedDictName = Literal["Movie"]

def define_types(
    type_var: TypeVarName,
    param_spec: ParamSpecName,
    type_var_tuple: TypeVarTupleName,
    new_type: NewTypeName,
    type_alias: TypeAliasName,
    typed_dict: TypedDictName,
) -> None:
    T = TypeVar(type_var)  # ok
    P = ParamSpec(param_spec)  # ok
    Ts = TypeVarTuple(type_var_tuple)  # ok
    UserId = NewType(new_type, int)  # ok
    Number = TypeAliasType(type_alias, int)  # ok
    Movie = TypedDict(typed_dict, {"title": str})  # ok

    reveal_type(T.__name__)  # revealed: Literal["T"]
    reveal_type(P.__name__)  # revealed: Literal["P"]
    reveal_type(Ts.__name__)  # revealed: Literal["Ts"]
    reveal_type(Number.__name__)  # revealed: Literal["Number"]

    U = TypeVar(type_var)  # error: [mismatched-typevar-name] "Expected "U", got "T""
    reveal_type(U.__name__)  # revealed: Literal["T"]
```

## Type parameters named at runtime

When the name argument has type `str`, ty reports that the name match cannot be verified. The
assignment still defines a type parameter, and its runtime `__name__` has type `str`.

```py
from collections.abc import Callable
from typing_extensions import ParamSpec, TypeVar, TypeVarTuple

def runtime_name(name: str) -> str:
    return name

T = TypeVar(runtime_name("T"))  # error: [mismatched-typevar-name]
P = ParamSpec(name=runtime_name("P"))  # error: [mismatched-paramspec-name]
Ts = TypeVarTuple(runtime_name("Ts"))  # error: [mismatched-typevartuple-name]

reveal_type(T.__name__)  # revealed: str
reveal_type(P.__name__)  # revealed: str
reveal_type(Ts.__name__)  # revealed: str

def identity(value: T) -> T:
    return value

def invoke(callback: Callable[P, int], *args: P.args, **kwargs: P.kwargs) -> int:
    return callback(*args, **kwargs)

def keep_tuple(values: tuple[*Ts]) -> tuple[*Ts]:
    return values
```

## Non-string arguments

The factories reject name arguments with non-string types.

```py
from typing_extensions import NewType, ParamSpec, TypeAliasType, TypedDict, TypeVar, TypeVarTuple

UserId = NewType(42, int)  # error: [invalid-newtype]
Number = TypeAliasType(42, int)  # error: [invalid-type-alias-type]
Movie = TypedDict(42, {"title": str})  # error: [invalid-argument-type]
P = ParamSpec(42)  # error: [invalid-paramspec]
T = TypeVar(42)  # error: [invalid-legacy-type-variable]
Ts = TypeVarTuple(42)  # error: [invalid-legacy-type-variable]
```
