# Overloads

When ty evaluates the call of an overloaded function, it attempts to "match" the supplied arguments
with one or more overloads. This document describes the algorithm that it uses for overload
matching, which is the same as the one mentioned in the
[spec](https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation).

## Arity check

The first step is to perform arity check. The non-overloaded cases are described in the
[function](./function.md) document.

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def f() -> None: ...
@overload
def f(x: int) -> int: ...
```

```py
from overloaded import f

# These match a single overload
reveal_type(f())  # revealed: None
reveal_type(f(1))  # revealed: int

# error: [no-matching-overload] "No overload of function `f` matches arguments"
reveal_type(f("a", "b"))  # revealed: Unknown
```

## Type checking

The second step is to perform type checking. This is done for all the overloads that passed the
arity check.

### Single match

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...
@overload
def f(x: bytes) -> bytes: ...
```

Here, all of the calls below pass the arity check for all overloads, so we proceed to type checking
which filters out all but the matching overload:

```py
from overloaded import f

reveal_type(f(1))  # revealed: int
reveal_type(f("a"))  # revealed: str
reveal_type(f(b"b"))  # revealed: bytes
```

### Single match error

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def f() -> None: ...
@overload
def f(x: int) -> int: ...
@overload
def f(x: int, y: int) -> int: ...
```

If the arity check only matches a single overload, it should be evaluated as a regular
(non-overloaded) function call. This means that any diagnostics resulted during type checking that
call should be reported directly and not as a `no-matching-overload` error.

```py
from typing_extensions import reveal_type

from overloaded import f

reveal_type(f())  # revealed: None

# error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `Literal["a"]`"
reveal_type(f("a"))  # revealed: Unknown
```

More examples of this diagnostic can be found in the
[single_matching_overload.md](../diagnostics/single_matching_overload.md) document.

### Multiple matches

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B(A): ...

@overload
def f(x: A) -> A: ...
@overload
def f(x: B, y: int = 0) -> B: ...
```

```py
from overloaded import A, B, f

# These calls pass the arity check, and type checking matches both overloads:
reveal_type(f(A()))  # revealed: A
reveal_type(f(B()))  # revealed: A

# But, in this case, the arity check filters out the first overload, so we only have one match:
reveal_type(f(B(), 1))  # revealed: B
```

## Argument type expansion

This step is performed only if the previous steps resulted in **no matches**.

In this case, the algorithm would perform
[argument type expansion](https://typing.python.org/en/latest/spec/overload.html#argument-type-expansion)
and loops over from the type checking step, evaluating the argument lists.

### Expanding the only argument

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...
class C: ...

@overload
def f(x: A) -> A: ...
@overload
def f(x: B) -> B: ...
@overload
def f(x: C) -> C: ...
```

```py
from overloaded import A, B, C, f

def _(ab: A | B, ac: A | C, bc: B | C):
    reveal_type(f(ab))  # revealed: A | B
    reveal_type(f(bc))  # revealed: B | C
    reveal_type(f(ac))  # revealed: A | C
```

### Expanding first argument

If the set of argument lists created by expanding the first argument evaluates successfully, the
algorithm shouldn't expand the second argument.

`overloaded.pyi`:

```pyi
from typing import Literal, overload

class A: ...
class B: ...
class C: ...
class D: ...

@overload
def f(x: A, y: C) -> A: ...
@overload
def f(x: A, y: D) -> B: ...
@overload
def f(x: B, y: C) -> C: ...
@overload
def f(x: B, y: D) -> D: ...
```

```py
from overloaded import A, B, C, D, f

def _(a_b: A | B):
    reveal_type(f(a_b, C()))  # revealed: A | C
    reveal_type(f(a_b, D()))  # revealed: B | D

# But, if it doesn't, it should expand the second argument and try again:
def _(a_b: A | B, c_d: C | D):
    reveal_type(f(a_b, c_d))  # revealed: A | B | C | D
```

### Expanding second argument

If the first argument cannot be expanded, the algorithm should move on to the second argument,
keeping the first argument as is.

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...
class C: ...
class D: ...

@overload
def f(x: A, y: B) -> B: ...
@overload
def f(x: A, y: C) -> C: ...
@overload
def f(x: B, y: D) -> D: ...
```

```py
from overloaded import A, B, C, D, f

def _(a: A, bc: B | C, cd: C | D):
    # This also tests that partial matching works correctly as the argument type expansion results
    # in matching the first and second overloads, but not the third one.
    reveal_type(f(a, bc))  # revealed: B | C

    # error: [no-matching-overload] "No overload of function `f` matches arguments"
    reveal_type(f(a, cd))  # revealed: Unknown
```

### Generics (legacy)

`overloaded.pyi`:

```pyi
from typing import TypeVar, overload

_T = TypeVar("_T")

class A: ...
class B: ...

@overload
def f(x: A) -> A: ...
@overload
def f(x: _T) -> _T: ...
```

```py
from overloaded import A, f

def _(x: int, y: A | int):
    reveal_type(f(x))  # revealed: int
    reveal_type(f(y))  # revealed: A | int
```

### Generics (PEP 695)

```toml
[environment]
python-version = "3.12"
```

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...

@overload
def f(x: B) -> B: ...
@overload
def f[T](x: T) -> T: ...
```

```py
from overloaded import B, f

def _(x: int, y: B | int):
    reveal_type(f(x))  # revealed: int
    reveal_type(f(y))  # revealed: B | int
```

### Expanding `bool`

`overloaded.pyi`:

```pyi
from typing import Literal, overload

class T: ...
class F: ...

@overload
def f(x: Literal[True]) -> T: ...
@overload
def f(x: Literal[False]) -> F: ...
```

```py
from overloaded import f

def _(flag: bool):
    reveal_type(f(True))  # revealed: T
    reveal_type(f(False))  # revealed: F
    reveal_type(f(flag))  # revealed: T | F
```

### Expanding `tuple`

`overloaded.pyi`:

```pyi
from typing import Literal, overload

class A: ...
class B: ...
class C: ...
class D: ...

@overload
def f(x: tuple[A, int], y: tuple[int, Literal[True]]) -> A: ...
@overload
def f(x: tuple[A, int], y: tuple[int, Literal[False]]) -> B: ...
@overload
def f(x: tuple[B, int], y: tuple[int, Literal[True]]) -> C: ...
@overload
def f(x: tuple[B, int], y: tuple[int, Literal[False]]) -> D: ...
```

```py
from overloaded import A, B, f

def _(x: tuple[A | B, int], y: tuple[int, bool]):
    reveal_type(f(x, y))  # revealed: A | B | C | D
```

### Expanding `type`

There's no special handling for expanding `type[A | B]` type because ty stores this type in it's
distributed form, which is `type[A] | type[B]`.

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...

@overload
def f(x: type[A]) -> A: ...
@overload
def f(x: type[B]) -> B: ...
```

```py
from overloaded import A, B, f

def _(x: type[A | B]):
    reveal_type(x)  # revealed: type[A] | type[B]
    reveal_type(f(x))  # revealed: A | B
```

### Expanding enums

`overloaded.pyi`:

```pyi
from enum import Enum
from typing import Literal, overload

class SomeEnum(Enum):
    A = 1
    B = 2
    C = 3


class A: ...
class B: ...
class C: ...

@overload
def f(x: Literal[SomeEnum.A]) -> A: ...
@overload
def f(x: Literal[SomeEnum.B]) -> B: ...
@overload
def f(x: Literal[SomeEnum.C]) -> C: ...
```

```py
from overloaded import SomeEnum, A, B, C, f

def _(x: SomeEnum):
    reveal_type(f(SomeEnum.A))  # revealed: A
    # TODO: This should be `B` once enums are supported and are expanded
    reveal_type(f(SomeEnum.B))  # revealed: A
    # TODO: This should be `C` once enums are supported and are expanded
    reveal_type(f(SomeEnum.C))  # revealed: A
    # TODO: This should be `A | B | C` once enums are supported and are expanded
    reveal_type(f(x))  # revealed: A
```

### No matching overloads

> If argument expansion has been applied to all arguments and one or more of the expanded argument
> lists cannot be evaluated successfully, generate an error and stop.

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...
class C: ...
class D: ...

@overload
def f(x: A) -> A: ...
@overload
def f(x: B) -> B: ...
```

```py
from overloaded import A, B, C, D, f

def _(ab: A | B, ac: A | C, cd: C | D):
    reveal_type(f(ab))  # revealed: A | B

    # The `[A | C]` argument list is expanded to `[A], [C]` where the first list matches the first
    # overload while the second list doesn't match any of the overloads, so we generate an
    # error: [no-matching-overload] "No overload of function `f` matches arguments"
    reveal_type(f(ac))  # revealed: Unknown

    # None of the expanded argument lists (`[C], [D]`) match any of the overloads, so we generate an
    # error: [no-matching-overload] "No overload of function `f` matches arguments"
    reveal_type(f(cd))  # revealed: Unknown
```

## Filtering overloads with variadic arguments and parameters

TODO

## Filtering based on `Any` / `Unknown`

This is the step 5 of the overload call evaluation algorithm which specifies that:

> For all arguments, determine whether all possible materializations of the argument’s type are
> assignable to the corresponding parameter type for each of the remaining overloads. If so,
> eliminate all of the subsequent remaining overloads.

This is only performed if the previous step resulted in more than one matching overload.

### Single list argument

`overloaded.pyi`:

```pyi
from typing import Any, overload

@overload
def f(x: list[int]) -> int: ...
@overload
def f(x: list[Any]) -> int: ...
@overload
def f(x: Any) -> str: ...
```

For the above definition, anything other than `list` should match the last overload:

```py
from typing import Any

from overloaded import f

# Anything other than `list` should match the last overload
reveal_type(f(1))  # revealed: str

def _(list_int: list[int], list_any: list[Any]):
    reveal_type(f(list_int))  # revealed: int
    reveal_type(f(list_any))  # revealed: int
```

### Single list argument (ambiguous)

The overload definition is the same as above, but the return type of the second overload is changed
to `str` to make the overload matching ambiguous if the argument is a `list[Any]`.

`overloaded.pyi`:

```pyi
from typing import Any, overload

@overload
def f(x: list[int]) -> int: ...
@overload
def f(x: list[Any]) -> str: ...
@overload
def f(x: Any) -> str: ...
```

```py
from typing import Any

from overloaded import f

# Anything other than `list` should match the last overload
reveal_type(f(1))  # revealed: str

def _(list_int: list[int], list_any: list[Any]):
    # All materializations of `list[int]` are assignable to `list[int]`, so it matches the first
    # overload.
    reveal_type(f(list_int))  # revealed: int

    # All materializations of `list[Any]` are assignable to `list[int]` and `list[Any]`, but the
    # return type of first and second overloads are not equivalent, so the overload matching
    # is ambiguous.
    reveal_type(f(list_any))  # revealed: Unknown
```

### Single tuple argument

`overloaded.pyi`:

```pyi
from typing import Any, overload

@overload
def f(x: tuple[int, str]) -> int: ...
@overload
def f(x: tuple[int, Any]) -> int: ...
@overload
def f(x: Any) -> str: ...
```

```py
from typing import Any

from overloaded import f

reveal_type(f("a"))  # revealed: str
reveal_type(f((1, "b")))  # revealed: int
reveal_type(f((1, 2)))  # revealed: int

def _(int_str: tuple[int, str], int_any: tuple[int, Any], any_any: tuple[Any, Any]):
    # All materializations are assignable to first overload, so second and third overloads are
    # eliminated
    reveal_type(f(int_str))  # revealed: int

    # All materializations are assignable to second overload, so the third overload is eliminated;
    # the return type of first and second overload is equivalent
    reveal_type(f(int_any))  # revealed: int

    # All materializations of `tuple[Any, Any]` are assignable to the parameters of all the
    # overloads, but the return types aren't equivalent, so the overload matching is ambiguous
    reveal_type(f(any_any))  # revealed: Unknown
```

### Multiple arguments

`overloaded.pyi`:

```pyi
from typing import Any, overload

class A: ...
class B: ...

@overload
def f(x: list[int], y: tuple[int, str]) -> A: ...
@overload
def f(x: list[Any], y: tuple[int, Any]) -> A: ...
@overload
def f(x: list[Any], y: tuple[Any, Any]) -> B: ...
```

```py
from typing import Any

from overloaded import A, f

def _(list_int: list[int], list_any: list[Any], int_str: tuple[int, str], int_any: tuple[int, Any], any_any: tuple[Any, Any]):
    # All materializations of both argument types are assignable to the first overload, so the
    # second and third overloads are filtered out
    reveal_type(f(list_int, int_str))  # revealed: A

    # All materialization of first argument is assignable to first overload and for the second
    # argument, they're assignable to the second overload, so the third overload is filtered out
    reveal_type(f(list_int, int_any))  # revealed: A

    # All materialization of first argument is assignable to second overload and for the second
    # argument, they're assignable to the first overload, so the third overload is filtered out
    reveal_type(f(list_any, int_str))  # revealed: A

    # All materializations of both arguments are assignable to the second overload, so the third
    # overload is filtered out
    reveal_type(f(list_any, int_any))  # revealed: A

    # All materializations of first argument is assignable to the second overload and for the second
    # argument, they're assignable to the third overload, so no overloads are filtered out; the
    # return types of the remaining overloads are not equivalent, so overload matching is ambiguous
    reveal_type(f(list_int, any_any))  # revealed: Unknown
```

### `LiteralString` and `str`

`overloaded.pyi`:

```pyi
from typing import overload
from typing_extensions import LiteralString

@overload
def f(x: LiteralString) -> LiteralString: ...
@overload
def f(x: str) -> str: ...
```

```py
from typing import Any
from typing_extensions import LiteralString

from overloaded import f

def _(literal: LiteralString, string: str, any: Any):
    reveal_type(f(literal))  # revealed: LiteralString
    reveal_type(f(string))  # revealed: str

    # `Any` matches both overloads, but the return types are not equivalent.
    # Pyright and mypy both reveal `str` here, contrary to the spec.
    reveal_type(f(any))  # revealed: Unknown
```

### Generics

`overloaded.pyi`:

```pyi
from typing import Any, TypeVar, overload

_T = TypeVar("_T")

class A: ...
class B: ...

@overload
def f(x: list[int]) -> A: ...
@overload
def f(x: list[_T]) -> _T: ...
@overload
def f(x: Any) -> B: ...
```

```py
from typing import Any

from overloaded import f

def _(list_int: list[int], list_str: list[str], list_any: list[Any], any: Any):
    reveal_type(f(list_int))  # revealed: A
    # TODO: Should be `str`
    reveal_type(f(list_str))  # revealed: Unknown
    reveal_type(f(list_any))  # revealed: Unknown
    reveal_type(f(any))  # revealed: Unknown
```

### Generics (multiple arguments)

`overloaded.pyi`:

```pyi
from typing import Any, TypeVar, overload

_T = TypeVar("_T")

@overload
def f(x: int, y: Any) -> int: ...
@overload
def f(x: str, y: _T) -> _T: ...
```

```py
from typing import Any

from overloaded import f

def _(integer: int, string: str, any: Any, list_any: list[Any]):
    reveal_type(f(integer, string))  # revealed: int
    reveal_type(f(string, integer))  # revealed: int

    # This matches the second overload and is _not_ the case of ambiguous overload matching.
    reveal_type(f(string, any))  # revealed: Any

    reveal_type(f(string, list_any))  # revealed: list[Any]
```

### Generic `self`

`overloaded.pyi`:

```pyi
from typing import Any, overload, TypeVar, Generic

_T = TypeVar("_T")

class A(Generic[_T]):
    @overload
    def method(self: "A[int]") -> int: ...
    @overload
    def method(self: "A[Any]") -> int: ...

class B(Generic[_T]):
    @overload
    def method(self: "B[int]") -> int: ...
    @overload
    def method(self: "B[Any]") -> str: ...
```

```py
from typing import Any

from overloaded import A, B

def _(a_int: A[int], a_str: A[str], a_any: A[Any]):
    reveal_type(a_int.method())  # revealed: int
    reveal_type(a_str.method())  # revealed: int
    reveal_type(a_any.method())  # revealed: int

def _(b_int: B[int], b_str: B[str], b_any: B[Any]):
    reveal_type(b_int.method())  # revealed: int
    reveal_type(b_str.method())  # revealed: str
    reveal_type(b_any.method())  # revealed: Unknown
```

### Variadic argument

TODO: A variadic parameter is being assigned to a number of parameters of the same type

### Non-participating fully-static parameter

Ref: <https://github.com/astral-sh/ty/issues/552#issuecomment-2969052173>

A non-participating parameter would be the one where the set of materializations of the argument
type, that are assignable to the parameter type at the same index, is same for the overloads for
which step 5 needs to be performed.

`overloaded.pyi`:

```pyi
from typing import Literal, overload

@overload
def f(x: str, *, flag: Literal[True]) -> int: ...
@overload
def f(x: str, *, flag: Literal[False] = ...) -> str: ...
@overload
def f(x: str, *, flag: bool = ...) -> int | str: ...
```

In the following example, for the `f(any, flag=True)` call, the materializations of first argument
type `Any` that are assignable to `str` is same for overloads 1 and 3 (at the time of step 5), so
for the purposes of overload matching that parameter can be ignored. If `Any` materializes to
anything that's not assignable to `str`, all of the overloads would already be filtered out which
will raise a `no-matching-overload` error.

```py
from typing import Any

from overloaded import f

def _(any: Any):
    reveal_type(f(any, flag=True))  # revealed: int
    reveal_type(f(any, flag=False))  # revealed: str
```

### Non-participating gradual parameter

`overloaded.pyi`:

```pyi
from typing import Any, Literal, overload

@overload
def f(x: tuple[str, Any], *, flag: Literal[True]) -> int: ...
@overload
def f(x: tuple[str, Any], *, flag: Literal[False] = ...) -> str: ...
@overload
def f(x: tuple[str, Any], *, flag: bool = ...) -> int | str: ...
```

```py
from typing import Any

from overloaded import f

def _(any: Any):
    reveal_type(f(any, flag=True))  # revealed: int
    reveal_type(f(any, flag=False))  # revealed: str
```

### Argument type expansion

This filtering can also happen for each of the expanded argument lists.

#### No ambiguity

`overloaded.pyi`:

```pyi
from typing import Any, overload

class A: ...
class B: ...

@overload
def f(x: tuple[A, B]) -> A: ...
@overload
def f(x: tuple[B, A]) -> B: ...
@overload
def f(x: tuple[A, Any]) -> A: ...
@overload
def f(x: tuple[B, Any]) -> B: ...
```

Here, the argument `tuple[A | B, Any]` doesn't match any of the overloads, so we perform argument
type expansion which results in two argument lists:

1. `tuple[A, Any]`
1. `tuple[B, Any]`

The first argument list matches overload 1 and 3 via `Any` materialization for which the return
types are equivalent (`A`). Similarly, the second argument list matches overload 2 and 4 via `Any`
materialization for which the return types are equivalent (`B`). The final return type for the call
will be the union of the return types.

```py
from typing import Any

from overloaded import A, B, f

def _(arg: tuple[A | B, Any]):
    reveal_type(f(arg))  # revealed: A | B
```

#### One argument list ambiguous

The example used here is same as the previous one, but the return type of the last overload is
changed so that it's not equivalent to the return type of the second overload, creating an ambiguous
matching for the second argument list.

`overloaded.pyi`:

```pyi
from typing import Any, overload

class A: ...
class B: ...
class C: ...

@overload
def f(x: tuple[A, B]) -> A: ...
@overload
def f(x: tuple[B, A]) -> B: ...
@overload
def f(x: tuple[A, Any]) -> A: ...
@overload
def f(x: tuple[B, Any]) -> C: ...
```

```py
from typing import Any

from overloaded import A, B, C, f

def _(arg: tuple[A | B, Any]):
    reveal_type(f(arg))  # revealed: A | Unknown
```

#### Both argument lists ambiguous

Here, both argument lists created by expanding the argument type are ambiguous, so the final return
type is `Any`.

`overloaded.pyi`:

```pyi
from typing import Any, overload

class A: ...
class B: ...
class C: ...

@overload
def f(x: tuple[A, B]) -> A: ...
@overload
def f(x: tuple[B, A]) -> B: ...
@overload
def f(x: tuple[A, Any]) -> C: ...
@overload
def f(x: tuple[B, Any]) -> C: ...
```

```py
from typing import Any

from overloaded import A, B, C, f

def _(arg: tuple[A | B, Any]):
    reveal_type(f(arg))  # revealed: Unknown
```
