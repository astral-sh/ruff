# Mismatched type names

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
