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

# The first string literal sets the quote style for the autofix below.
type TypeVarName = Literal['T']  # fmt: skip
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

    U = TypeVar(type_var)  # snapshot: mismatched-typevar-name

    reveal_type(U.__name__)  # revealed: Literal["T"]
```

```snapshot
warning[mismatched-typevar-name]: The name passed to `TypeVar` must match the variable it is assigned to
  --> src/mdtest_snippet.py:31:17
   |
31 |     U = TypeVar(type_var)  # snapshot: mismatched-typevar-name
   |                 ^^^^^^^^ Expected "U", got "T"
help: Use `'U'` as the name
   |
30 |
   -     U = TypeVar(type_var)  # snapshot: mismatched-typevar-name
31 +     U = TypeVar('U')  # snapshot: mismatched-typevar-name
32 |
   |
note: This is an unsafe fix and may change runtime behavior
```

## Type parameters named at runtime

When the name argument has type `str`, ty reports that the name match cannot be verified. The
assignment still defines a type parameter, and its runtime `__name__` has type `str`.

```py
from collections.abc import Callable
from typing_extensions import ParamSpec, TypeVar, TypeVarTuple

def runtime_name(name: str) -> str:
    return name
```

ty marks the fix as unsafe when it replaces a function call, since the call may have side effects.

```py
T = TypeVar(runtime_name("T"))  # snapshot: mismatched-typevar-name
```

```snapshot
warning[mismatched-typevar-name]: The name passed to `TypeVar` must match the variable it is assigned to
 --> src/mdtest_snippet.py:6:13
  |
6 | T = TypeVar(runtime_name("T"))  # snapshot: mismatched-typevar-name
  |             ^^^^^^^^^^^^^^^^^ Cannot verify that a value of type `str` matches "T"
help: Use `"T"` as the name
  |
5 |     return name
  - T = TypeVar(runtime_name("T"))  # snapshot: mismatched-typevar-name
6 + T = TypeVar("T")  # snapshot: mismatched-typevar-name
7 | P = ParamSpec(name=runtime_name("P"))  # error: [mismatched-paramspec-name]
  |
note: This is an unsafe fix and may change runtime behavior
```

```py
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

## Names in nested scopes

ty marks the fix as unsafe for definitions inside a class or function. For a type returned by a
factory function, pickle may need the module-level name the caller assigns to it. For the class
attribute below, pickle needs the qualified name `"C.Id"` to find the type from the module.

```py
from typing import NewType

class C:
    Id = NewType("C.Id", int)  # snapshot: mismatched-newtype-name
```

```snapshot
warning[mismatched-newtype-name]: The name passed to `NewType` must match the variable it is assigned to
 --> src/mdtest_snippet.py:4:18
  |
4 |     Id = NewType("C.Id", int)  # snapshot: mismatched-newtype-name
  |                  ^^^^^^ Expected "Id", got "C.Id"
help: Use `"Id"` as the name
  |
3 | class C:
  -     Id = NewType("C.Id", int)  # snapshot: mismatched-newtype-name
4 +     Id = NewType("Id", int)  # snapshot: mismatched-newtype-name
  |
note: This is an unsafe fix and may change runtime behavior
```

In a stub, changing the name has no effect on pickling, so the same fix is safe.

`models.pyi`:

```pyi
from typing import NewType

class C:
    Id = NewType("C.Id", int)  # snapshot: mismatched-newtype-name
```

```snapshot
warning[mismatched-newtype-name]: The name passed to `NewType` must match the variable it is assigned to
 --> src/models.pyi:4:18
  |
4 |     Id = NewType("C.Id", int)  # snapshot: mismatched-newtype-name
  |                  ^^^^^^ Expected "Id", got "C.Id"
help: Use `"Id"` as the name
  |
3 | class C:
  -     Id = NewType("C.Id", int)  # snapshot: mismatched-newtype-name
4 +     Id = NewType("Id", int)  # snapshot: mismatched-newtype-name
  |
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

## Fixing names split across string literals

ty replaces adjacent string literals with a single literal containing the variable name. The
replacement keeps the first literal's prefix. ty marks the fix as unsafe for all implicitly
concatenated strings because the replacement can remove comments between the literals.

```py
from typing import TypeVar

# snapshot: mismatched-typevar-name
T = TypeVar(name=r"T" "Value")
```

```snapshot
warning[mismatched-typevar-name]: The name passed to `TypeVar` must match the variable it is assigned to
 --> src/mdtest_snippet.py:4:18
  |
4 | T = TypeVar(name=r"T" "Value")
  |                  ^^^^^^^^^^^^ Expected "T", got "TValue"
help: Use `r"T"` as the name
  |
3 | # snapshot: mismatched-typevar-name
  - T = TypeVar(name=r"T" "Value")
4 + T = TypeVar(name=r"T")
  |
note: This is an unsafe fix and may change runtime behavior
```
