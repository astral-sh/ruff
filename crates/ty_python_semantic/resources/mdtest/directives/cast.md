# `cast`

## Behavior

```toml
[environment]
python-version = "3.12"

[rules]
# Disabled by default in production, but enabled by default in mdtests.
# Tests for this rule are lower down in the file; for this section, we disable the rule.
disjoint-cast = "ignore"
```

`cast()` takes two arguments, one type and one value, and returns a value of the given type.

The (inferred) type of the value and the given type do not need to have any correlation.

```py
from typing import Literal, cast, Any

reveal_type(True)  # revealed: Literal[True]
reveal_type(cast(str, True))  # revealed: str
reveal_type(cast("str", True))  # revealed: str

reveal_type(cast(int | str, 1))  # revealed: int | str

reveal_type(cast(val="foo", typ=int))  # revealed: int

# error: [invalid-type-form]
reveal_type(cast(Literal, True))  # revealed: Unknown

# error: [invalid-type-form]
reveal_type(cast(1, True))  # revealed: Unknown

# error: [missing-argument] "No argument provided for required parameter `val` of function `cast`"
cast(str)
# error: [too-many-positional-arguments] "Too many positional arguments to function `cast`: expected 2, got 3"
cast(str, b"ar", "foo")

def function_returning_int() -> int:
    return 10

# error: [redundant-cast] "Value is already of type `int`"
cast(int, function_returning_int())

def function_returning_any() -> Any:
    return "blah"

# error: [redundant-cast] "Value is already of type `Any`"
cast(Any, function_returning_any())
```

Complex type expressions (which may be unsupported) do not lead to spurious `[redundant-cast]`
diagnostics.

```py
from typing import Callable

def f(x: Callable[[dict[str, int]], None], y: tuple[dict[str, int]]):
    a = cast(Callable[[list[bytes]], None], x)
    b = cast(tuple[list[bytes]], y)
```

A cast from `Todo` or `Unknown` to `Any` is not considered a "redundant cast": even if these are
understood as gradually equivalent types by ty, they are understood as different types by human
readers of ty's output. For `Unknown` in particular, we may consider it differently in the context
of some opt-in diagnostics, as it indicates that the gradual type has come about due to an invalid
annotation, missing annotation or missing type argument somewhere.

A cast from `Unknown` to `Todo` or `Any` is also not considered a "redundant cast", as this breaks
the gradual guarantee and leads to cascading errors when an object is inferred as having type
`Unknown` due to a missing import or similar.

```py
from ty_extensions._internal import Unknown

def f(x: Any, y: Unknown, z: Any | str | int):
    a = cast(dict[str, Any], x)
    reveal_type(a)  # revealed: dict[str, Any]

    b = cast(Any, y)
    reveal_type(b)  # revealed: Any

    c = cast(Unknown, y)
    reveal_type(c)  # revealed: Unknown

    d = cast(Unknown, x)
    reveal_type(d)  # revealed: Unknown

    e = cast(str | int | Any, z)  # error: [redundant-cast]
```

Recursive aliases that fall back to `Divergent` should not trigger `redundant-cast`.

```py
from typing import cast

RecursiveAlias = list["RecursiveAlias | None"]

def f(x: RecursiveAlias):
    cast(RecursiveAlias, x)
```

## Redundant casts of tuple classes with unknown elements

A tuple class with an `Unknown` element is not fully static, even when its other element is `object`
and their union simplifies to `object`. A cast involving that tuple class must not be reported as
redundant.

```py
from typing import cast
from ty_extensions._internal import Unknown

def cast_gradual_tuple_class(value: type[tuple[object, Unknown]]) -> None:
    cast(type[tuple[object, Unknown]], value)
```

## Disjoint casts

### Basics

Casting between disjoint types often indicates a mistake in the user's code. When enabled,
`disjoint-cast` reports casts whose source and destination types have no overlap.

```py
from typing import cast
from typing_extensions import cast as extension_cast

def incompatible_casts(integer: int, string: str) -> None:
    # error: [disjoint-cast] "Cast from `int` to disjoint type `str`"
    cast(str, integer)

    # error: [disjoint-cast] "Cast from `str` to disjoint type `int`"
    cast(int, string)

    # error: [disjoint-cast] "Cast from `int` to disjoint type `str`"
    cast(val=integer, typ=str)

    # error: [disjoint-cast] "Cast from `int` to disjoint type `str`"
    extension_cast(str, integer)
```

### Disjoint casts involving literals and unions

Literal types and unions are rejected only when none of their possible values overlaps with the
destination type.

```py
from typing import Literal, cast

# error: [disjoint-cast] "Cast from `Literal[1]` to disjoint type `str`"
cast(str, 1)

# error: [disjoint-cast] "Cast from `Literal["left"]` to disjoint type `Literal["right"]`"
cast(Literal["right"], "left")

def cast_union(value: int | str) -> None:
    # error: [disjoint-cast] "Cast from `int | str` to disjoint type `bytes`"
    cast(bytes, value)

    cast(str, value)
    cast(int | bytes, value)
```

### Disjoint casts involving generic types

Incompatible generic specializations are rejected:

```py
from typing import Any, cast
from ty_extensions import Intersection

def cast_generic(
    list_of_integers: list[int],
    list_of_integers_or_any: list[int | Any],
    dynamic_list_of_integers: Intersection[list[int], Any],
    list_of_dynamic_integers: list[Intersection[int, Any]],
) -> None:
    # error: [disjoint-cast] "Cast from `list[int]` to disjoint type `list[str]`"
    cast(list[str], list_of_integers)
    # error: [disjoint-cast]
    cast(list[str], list_of_integers_or_any)
    # error: [disjoint-cast]
    cast(list[str], dynamic_list_of_integers)
    # error: [disjoint-cast]
    cast(list[str], list_of_dynamic_integers)
```

But `cast`s are permitted between two different specializations of the same invariant generic type
when those two different specializations overlap. This can occur with certain dynamic
specializations of invariant generics:

```py
def cast_generic_invalid(
    list_of_integers: list[int],
    dynamic_list_of_integers: Intersection[list[int], Any],
    list_of_dynamic_integers: list[Intersection[int, Any]],
    list_of_integers_or_any: list[int | Any],
    any_or_list_of_integers: list[int] | Any,
    any_or_list_of_integers_or_any: list[int | Any] | Any,
    list_of_any: list[Any],
    just_any: Any,
):
    cast(Any, list_of_integers)  # no diagnostic
    cast(list[str], list_of_any)  # no diagnostic
    cast(str, just_any)  # no diagnostic
    cast(list[Intersection[int, Any]], list_of_integers)  # no diagnostic
    cast(list[int], dynamic_list_of_integers)  # no diagnostic
    cast(list[int], list_of_dynamic_integers)  # no diagnostic
    cast(list[int], list_of_integers_or_any)  # no diagnostic
    cast(list[int], any_or_list_of_integers)  # no diagnostic

    # `Any | list[int]` could materialize to `list[str] | list[int]`,
    # which is not disjoint from `list[str]`
    cast(list[str], any_or_list_of_integers)  # no diagnostic

    # similarly `Any | list[int | Any]` could also materialize to `list[str] | list[int]`
    cast(list[str], any_or_list_of_integers_or_any)  # no diagnostic
```

### Disjoint casts between identically named types

Disjoint types with the same display name are qualified so the diagnostic identifies which type
comes from each module.

```py
from typing import cast

import first
import second

def cast_identically_named(value: first.Value) -> None:
    # error: [disjoint-cast] "Cast from `first.Value` to disjoint type `second.Value`"
    cast(second.Value, value)
```

`first.py`:

```py
from typing import final

@final
class Value:
    pass
```

`second.py`:

```py
from typing import final

@final
class Value:
    pass
```

### Invariant type arguments in disjoint-cast explanations

Distinct specializations of an invariant container are disjoint even when their element types
overlap. The explanation identifies the invariant parameter and the failed subtype check.

```py
from typing import cast

def narrow_elements(values: list[int | str]) -> None:
    # snapshot: disjoint-cast
    cast(list[int], values)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
    --> src/mdtest_snippet.py:5:5
     |
   5 |     cast(list[int], values)
     |     ^^^^^---------^^------^
     |          |          |
     |          |          Inferred as `list[int | str]`
     |          Disjoint from the inferred type
     |
    ::: stdlib/builtins.pyi:2966:7
     |
2966 | class list(MutableSequence[_T]):
     |       ---- `list` defined here
info: `list[int]` is disjoint from `list[int | str]`
info: `int | str` and `int` are not mutual subtypes of each other, but must be due to invariance
info: └── element `str` of union `int | str` is not a subtype of `int`
```

### Nominal subclasses of protocols

Inheriting from a protocol does not make a class a protocol. A final class can satisfy the protocol
structurally, but cannot also be an instance of its unrelated nominal subclass.

```py
from typing import Protocol, cast, final

class HasName(Protocol):
    name: str

class Named(HasName):
    pass

@final
class Function:
    name: str

def cast_function(function: Function) -> None:
    cast(HasName, function)

    # snapshot: disjoint-cast
    cast(Named, function)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
  --> src/mdtest_snippet.py:17:5
   |
17 |       cast(Named, function)
   |       ^^^^^-----^^--------^
   |            |      |
   |            |      Inferred as `Function`
   |            Disjoint from the inferred type
   |
  ::: src/mdtest_snippet.py:6:7
   |
 6 |   class Named(HasName):
   |         ----- `Named` defined here
 7 |       pass
 8 |
 9 | / @final
10 | | class Function:
   | |______________- `Function` defined here
info: `Named` is disjoint from `Function`
info: `Function` is `@final` and not a subclass of `Named`
```

### Explaining every disjoint union element

A union is disjoint from the destination only when every element is disjoint. Each element
contributes its own explanation.

```py
from typing import cast

def cast_union(value: list[str] | list[bytes]) -> None:
    # snapshot: disjoint-cast
    cast(list[int], value)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
    --> src/mdtest_snippet.py:5:5
     |
   5 |     cast(list[int], value)
     |     ^^^^^---------^^-----^
     |          |          |
     |          |          Inferred as `list[str] | list[bytes]`
     |          Disjoint from the inferred type
     |
    ::: stdlib/builtins.pyi:2966:7
     |
2966 | class list(MutableSequence[_T]):
     |       ---- `list` defined here
info: `list[int]` is disjoint from `list[str] | list[bytes]`
info: every element of union `list[str] | list[bytes]` is disjoint from `list[int]`
info: ├── `str` and `int` are not mutual subtypes of each other, but must be due to invariance
info: └── `bytes` and `int` are not mutual subtypes of each other, but must be due to invariance
```

### Disjoint tuple elements

Two tuples of the same length are disjoint when a required element has disjoint types. The
explanation identifies the position of that element.

```py
from typing import cast

def cast_tuple(value: tuple[int, str]) -> None:
    # snapshot: disjoint-cast
    cast(tuple[int, int], value)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
    --> src/mdtest_snippet.py:5:5
     |
   5 |     cast(tuple[int, int], value)
     |     ^^^^^---------------^^-----^
     |          |                |
     |          |                Inferred as `tuple[int, str]`
     |          Disjoint from the inferred type
     |
    ::: stdlib/builtins.pyi:2851:7
     |
2851 | class tuple(Sequence[_T_co]):
     |       ----- `tuple` defined here
info: `tuple[int, int]` is disjoint from `tuple[int, str]`
info: tuple element 2 has disjoint types `str` and `int`
info: └── `str` and `int` are disjoint due to incompatible instance layouts
```

### Disjoint tuple lengths

A fixed-length tuple cannot overlap with a tuple that requires more elements.

```py
from typing import cast

def cast_tuple(value: tuple[int]) -> None:
    # snapshot: disjoint-cast
    cast(tuple[int, int], value)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
    --> src/mdtest_snippet.py:5:5
     |
   5 |     cast(tuple[int, int], value)
     |     ^^^^^---------------^^-----^
     |          |                |
     |          |                Inferred as `tuple[int]`
     |          Disjoint from the inferred type
     |
    ::: stdlib/builtins.pyi:2851:7
     |
2851 | class tuple(Sequence[_T_co]):
     |       ----- `tuple` defined here
info: `tuple[int, int]` is disjoint from `tuple[int]`
info: the tuples have incompatible lengths: 1 and 2
```

### Missing protocol members

A missing member makes a final class disjoint from a protocol. The explanation does not retain
unsuccessful attempts to prove that an earlier, compatible member is disjoint.

```py
from typing import Protocol, cast, final

class Target(Protocol):
    compatible: int | str
    missing: str

@final
class Source:
    compatible: int

def cast_protocol(value: Source) -> None:
    # snapshot: disjoint-cast
    cast(Target, value)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
  --> src/mdtest_snippet.py:13:5
   |
13 |       cast(Target, value)
   |       ^^^^^------^^-----^
   |            |       |
   |            |       Inferred as `Source`
   |            Disjoint from the inferred type
   |
  ::: src/mdtest_snippet.py:3:7
   |
 3 |   class Target(Protocol):
   |         ------ `Target` defined here
 4 |       compatible: int | str
 5 |       missing: str
 6 |
 7 | / @final
 8 | | class Source:
   | |____________- `Source` defined here
info: protocol `Target` is disjoint from `Source`
info: `@final` type `Source` does not provide all members of protocol `Target`
info: └── protocol member `missing` is not defined on type `Source`
```

The same explanation applies when casting from the protocol to the final class.

```py
def cast_final(value: Target) -> None:
    # snapshot: disjoint-cast
    cast(Source, value)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
  --> src/mdtest_snippet.py:16:5
   |
16 |       cast(Source, value)
   |       ^^^^^------^^-----^
   |            |       |
   |            |       Inferred as `Target`
   |            Disjoint from the inferred type
   |
  ::: src/mdtest_snippet.py:3:7
   |
 3 |   class Target(Protocol):
   |         ------ `Target` defined here
 4 |       compatible: int | str
 5 |       missing: str
 6 |
 7 | / @final
 8 | | class Source:
   | |____________- `Source` defined here
info: `Source` is disjoint from protocol `Target`
info: `@final` type `Source` does not provide all members of protocol `Target`
info: └── protocol member `missing` is not defined on type `Source`
```

### Disjoint protocol method returns

The explanation follows the protocol member's return type into its incompatible generic
specialization.

```py
from typing import Protocol, cast, final

class Target(Protocol):
    def values(self) -> list[int]: ...

@final
class Source:
    def values(self) -> list[str]:
        return []

def cast_protocol(value: Source) -> None:
    # snapshot: disjoint-cast
    cast(Target, value)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
  --> src/mdtest_snippet.py:13:5
   |
13 |       cast(Target, value)
   |       ^^^^^------^^-----^
   |            |       |
   |            |       Inferred as `Source`
   |            Disjoint from the inferred type
   |
  ::: src/mdtest_snippet.py:3:7
   |
 3 |   class Target(Protocol):
   |         ------ `Target` defined here
 4 |       def values(self) -> list[int]: ...
 5 |
 6 | / @final
 7 | | class Source:
   | |____________- `Source` defined here
info: protocol `Target` is disjoint from `Source`
info: protocol member `values` is incompatible
info: └── return types `list[int]` and `list[str]` are disjoint
info:     └── `int` and `str` are not mutual subtypes of each other, but must be due to invariance
```

### Disjoint mutable TypedDict fields

Mutable fields must accept assignments in both directions. The explanation identifies the field and
the failed assignability check, rather than a subtype check.

```py
from typing import TypedDict, cast

class Source(TypedDict):
    value: int | str

class Target(TypedDict):
    value: int

def cast_fields(value: Source) -> None:
    # snapshot: disjoint-cast
    cast(Target, value)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
  --> src/mdtest_snippet.py:11:5
   |
11 |     cast(Target, value)
   |     ^^^^^------^^-----^
   |          |       |
   |          |       Inferred as `Source`
   |          Disjoint from the inferred type
   |
  ::: src/mdtest_snippet.py:3:7
   |
 3 | class Source(TypedDict):
   |       ------ `Source` defined here
 4 |     value: int | str
 5 |
 6 | class Target(TypedDict):
   |       ------ `Target` defined here
info: `Target` is disjoint from `Source`
info: field `value` has incompatible types `int | str` and `int`
info: └── element `str` of union `int | str` is not assignable to `int`
```

### Conflicting TypedDict requiredness

A required field cannot also be a mutable optional field: the optional declaration permits deleting
it.

```py
from typing import TypedDict, cast
from typing_extensions import NotRequired

class Source(TypedDict):
    value: int

class Target(TypedDict):
    value: NotRequired[int]

def cast_fields(value: Source) -> None:
    # snapshot: disjoint-cast
    cast(Target, value)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
  --> src/mdtest_snippet.py:12:5
   |
12 |     cast(Target, value)
   |     ^^^^^------^^-----^
   |          |       |
   |          |       Inferred as `Source`
   |          Disjoint from the inferred type
   |
  ::: src/mdtest_snippet.py:4:7
   |
 4 | class Source(TypedDict):
   |       ------ `Source` defined here
 5 |     value: int
 6 |
 7 | class Target(TypedDict):
   |       ------ `Target` defined here
info: `Target` is disjoint from `Source`
info: field `value` is required in `Source` but mutable and not-required in `Target`
```

Reversing the cast does not change which TypedDict requires the field.

```py
def cast_required(value: Target) -> None:
    # snapshot: disjoint-cast
    cast(Source, value)
```

```snapshot
info[disjoint-cast]: Cast to a disjoint type
  --> src/mdtest_snippet.py:15:5
   |
15 |     cast(Source, value)
   |     ^^^^^------^^-----^
   |          |       |
   |          |       Inferred as `Target`
   |          Disjoint from the inferred type
   |
  ::: src/mdtest_snippet.py:4:7
   |
 4 | class Source(TypedDict):
   |       ------ `Source` defined here
 5 |     value: int
 6 |
 7 | class Target(TypedDict):
   |       ------ `Target` defined here
info: `Source` is disjoint from `Target`
info: field `value` is required in `Source` but mutable and not-required in `Target`
```

### Casts to `Never`

`Never` is disjoint from every type, but excluded from `disjoint-cast`. It is assumed that the user
knows what they're doing if they cast to `Never` explicitly:

```py
from typing_extensions import Never, cast

x = cast(Never, 0)  # no diagnostic
```

Upcasts from a `Never`-inferred type to a supertype are also permitted without the rule being
triggered:

```py
from typing_extensions import Never, cast

def test(x: Never):
    y = cast(str, x)  # no diagnostic
```

The reason why casting to or from `Never` is allowed is that the normal rationale for this rule does
not apply to either case.

This rule seeks to prevent you from `cast`ing from a type `X` to a type `Y` if ty would never
provide any way for you to soundly narrow a type `X` to a type `Y`. Casting from `Never` to any
other type, however, poses no soundness issues: all types are supertypes of `Never`, so this can
never be unsound. Meanwhile, casting from `int` to `Never` is unsound, of course, but not really in
a different category than casting from `int` to `bool` (which would still be allowed under this
rule). `Never` is a subtype of `int` just the same way that `bool` is a subtype of `int`, and there
are lots of mechanisms ty provides that would let you soundly narrow a type from `int` to `Never`
without using a `cast`.

### Casts in stub files

`disjoint-cast` is not applied to stub files:

`stub.pyi`:

```pyi
from typing import cast

x = cast(int, ...)  # no diagnostic
```

This is partly to accommodate the fact that the typing spec
[recommends](https://typing.python.org/en/latest/spec/enums.html#defining-members) using
`cast(<value type>, ...)` to declare enum members in stub files in cases where the type of the
member's value cannot be unambiguously expressed as a static assignment. Without a special case for
stubs, we would emit a false-positive diagnostic on this example from the spec. This is due to the
fact that `EllipsisType` (the type of `...`) is disjoint from almost every other type, since it is
`@final`:

`stub2.pyi`:

```pyi
from enum import Enum
from typing import cast

class Pet(Enum):
    genus: str  # Non-member attribute
    species: str  # Non-member attribute

    CAT = 1  # Member attribute with known value and type
    DOG = cast(int, ...)  # Member attribute with unknown value and known type
    BIRD = ...  # Member attribute with unknown value and type
```

But the rule would also serve little purpose in stub files. Since stub files are never executed at
runtime, the only possible useful applications of `cast` in a stub file are special cases like the
enum one above. There are no soundness implications to using `cast` in a stub file.

For similar reasons, we also do not apply the rule in `if TYPE_CHECKING` blocks, which are also not
executed at runtime:

`regular_py_file.py`:

```py
from typing import TYPE_CHECKING, cast

if TYPE_CHECKING:
    x = cast(int, ...)  # no diagnostic
```

## Diagnostic snapshots

```py
import secrets
from typing import cast

# snapshot: redundant-cast
cast(int, secrets.randbelow(10))
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
 --> src/mdtest_snippet.py:5:1
  |
5 | cast(int, secrets.randbelow(10))
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove the redundant `cast`
  |
4 | # snapshot: redundant-cast
  - cast(int, secrets.randbelow(10))
5 + secrets.randbelow(10)
6 | # snapshot: redundant-cast
  |
```

```py
# snapshot: redundant-cast
cast(val=secrets.randbelow(10), typ=int)
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
 --> src/mdtest_snippet.py:7:1
  |
7 | cast(val=secrets.randbelow(10), typ=int)
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove the redundant `cast`
  |
6 | # snapshot: redundant-cast
  - cast(val=secrets.randbelow(10), typ=int)
7 + secrets.randbelow(10)
8 | def f(x: int, y: int, z: int) -> int:
  |
```

```py
def f(x: int, y: int, z: int) -> int:
    # snapshot: redundant-cast
    return cast(int, x + y) * z
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
  --> src/mdtest_snippet.py:10:12
   |
10 |     return cast(int, x + y) * z
   |            ^^^^^^^^^^^^^^^^
help: Remove the redundant `cast`
   |
9  |     # snapshot: redundant-cast
   -     return cast(int, x + y) * z
10 +     return (x + y) * z
11 | def g(x: int, y: int) -> int:
   |
```

```py
def g(x: int, y: int) -> int:
    # snapshot: redundant-cast
    return -cast(int, x + y)
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
  --> src/mdtest_snippet.py:13:13
   |
13 |     return -cast(int, x + y)
   |             ^^^^^^^^^^^^^^^^
help: Remove the redundant `cast`
   |
12 |     # snapshot: redundant-cast
   -     return -cast(int, x + y)
13 +     return -(x + y)
14 | def h(x: int, y: int) -> None:
   |
```

```py
def h(x: int, y: int) -> None:
    # snapshot: redundant-cast
    print(cast(int, x + y))
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
  --> src/mdtest_snippet.py:16:11
   |
16 |     print(cast(int, x + y))
   |           ^^^^^^^^^^^^^^^^
help: Remove the redundant `cast`
   |
15 |     # snapshot: redundant-cast
   -     print(cast(int, x + y))
16 +     print(x + y)
   |
```

## Fixes for multiline conditional expressions

Removing a redundant cast preserves the parentheses that allow its argument to span multiple lines.

```py
from typing import cast

# fmt: off
def choose(x: int, y: int, flag: bool) -> int:
    # snapshot: redundant-cast
    return cast(int, (x if flag
                     else y))
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
 --> src/mdtest_snippet.py:6:12
  |
6 |       return cast(int, (x if flag
  |  ____________^
7 | |                      else y))
  | |_____________________________^
help: Remove the redundant `cast`
  |
5 |     # snapshot: redundant-cast
  -     return cast(int, (x if flag
  -                      else y))
6 +     return (x if flag
7 +                      else y)
  |
```

## Fixes for multiline arithmetic expressions

An argument can rely on the call's parentheses for line continuation without having parentheses of
its own. Removing the call adds parentheses to keep the arithmetic expression on one logical line.

```py
from typing import cast

# fmt: off
def add(x: int, y: int) -> int:
    # snapshot: redundant-cast
    return cast(int, x +
                    y)
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
 --> src/mdtest_snippet.py:6:12
  |
6 |       return cast(int, x +
  |  ____________^
7 | |                     y)
  | |______________________^
help: Remove the redundant `cast`
  |
5 |     # snapshot: redundant-cast
  -     return cast(int, x +
6 +     return (x +
7 |                     y)
  |
```

A line break before an operator also needs parentheses. Without them, the following fix would
produce valid syntax but return only `x`, leaving `+ y` as an unreachable statement.

```py
# fmt: off
def add_with_leading_operator(x: int, y: int) -> int:
    # snapshot: redundant-cast
    return cast(int, x
    + y)
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
  --> src/mdtest_snippet.py:11:12
   |
11 |       return cast(int, x
   |  ____________^
12 | |     + y)
   | |________^
help: Remove the redundant `cast`
   |
10 |     # snapshot: redundant-cast
   -     return cast(int, x
11 +     return (x
12 |     + y)
   |
```

## Fixes preserve comments in parenthesized arguments

The fix retains comments inside an argument's parentheses, including when the value is passed by
keyword before the type argument.

```py
from typing import cast

def add(x: int, y: int) -> int:
    # snapshot: redundant-cast
    return cast(
        val=(
            # Leading comment.
            x + y  # Trailing comment.
        ),
        typ=int,
    )
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
  --> src/mdtest_snippet.py:5:12
   |
 5 |       return cast(
   |  ____________^
 6 | |         val=(
 7 | |             # Leading comment.
 8 | |             x + y  # Trailing comment.
 9 | |         ),
10 | |         typ=int,
11 | |     )
   | |_____^
help: Remove the redundant `cast`
  |
4 |     # snapshot: redundant-cast
  -     return cast(
  -         val=(
5 +     return (
6 |             # Leading comment.
7 |             x + y  # Trailing comment.
  -         ),
  -         typ=int,
  -     )
8 +         )
  |
```
