# Overloads

When ty evaluates the call of an overloaded function, it attempts to "match" the supplied arguments
with one or more overloads. This document describes the algorithm that it uses for overload
matching, which is the same as the one mentioned in the
[spec](https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation).

Note that all of the examples that involve positional parameters are tested multiple times: once
with the parameters matched with individual positional arguments, and once with the parameters
matched with a single positional argument that is splatted into the argument list. Overload
resolution is performed _after_ splatted arguments have been expanded, and so both approaches (TODO:
should) produce the same results.

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
reveal_type(f(*()))  # revealed: None

reveal_type(f(1))  # revealed: int
reveal_type(f(*(1,)))  # revealed: int

# error: [no-matching-overload] "No overload of function `f` matches arguments"
reveal_type(f("a", "b"))  # revealed: Unknown
# error: [no-matching-overload] "No overload of function `f` matches arguments"
reveal_type(f(*("a", "b")))  # revealed: Unknown
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
reveal_type(f(*(1,)))  # revealed: int

reveal_type(f("a"))  # revealed: str
reveal_type(f(*("a",)))  # revealed: str

reveal_type(f(b"b"))  # revealed: bytes
reveal_type(f(*(b"b",)))  # revealed: bytes
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
from overloaded import f

reveal_type(f())  # revealed: None
reveal_type(f(*()))  # revealed: None

# error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `Literal["a"]`"
reveal_type(f("a"))  # revealed: Unknown
# error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `Literal["a"]`"
reveal_type(f(*("a",)))  # revealed: Unknown
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
reveal_type(f(*(A(),)))  # revealed: A

reveal_type(f(B()))  # revealed: A
reveal_type(f(*(B(),)))  # revealed: A

# But, in this case, the arity check filters out the first overload, so we only have one match:
reveal_type(f(B(), 1))  # revealed: B
reveal_type(f(*(B(), 1)))  # revealed: B
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
    reveal_type(f(*(ab,)))  # revealed: A | B

    reveal_type(f(bc))  # revealed: B | C
    reveal_type(f(*(bc,)))  # revealed: B | C

    reveal_type(f(ac))  # revealed: A | C
    reveal_type(f(*(ac,)))  # revealed: A | C
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
    reveal_type(f(*(a_b, C())))  # revealed: A | C

    reveal_type(f(a_b, D()))  # revealed: B | D
    reveal_type(f(*(a_b, D())))  # revealed: B | D

# But, if it doesn't, it should expand the second argument and try again:
def _(a_b: A | B, c_d: C | D):
    reveal_type(f(a_b, c_d))  # revealed: A | B | C | D
    reveal_type(f(*(a_b, c_d)))  # revealed: A | B | C | D
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
    reveal_type(f(*(a, bc)))  # revealed: B | C

    # error: [no-matching-overload] "No overload of function `f` matches arguments"
    reveal_type(f(a, cd))  # revealed: Unknown
    # error: [no-matching-overload] "No overload of function `f` matches arguments"
    reveal_type(f(*(a, cd)))  # revealed: Unknown
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
    reveal_type(f(*(x,)))  # revealed: int

    reveal_type(f(y))  # revealed: A | int
    reveal_type(f(*(y,)))  # revealed: A | int
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
    reveal_type(f(*(x,)))  # revealed: int

    reveal_type(f(y))  # revealed: B | int
    reveal_type(f(*(y,)))  # revealed: B | int
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
    reveal_type(f(*(True,)))  # revealed: T

    reveal_type(f(False))  # revealed: F
    reveal_type(f(*(False,)))  # revealed: F

    reveal_type(f(flag))  # revealed: T | F
    reveal_type(f(*(flag,)))  # revealed: T | F
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
    reveal_type(f(*(x, y)))  # revealed: A | B | C | D
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
    reveal_type(f(*(x,)))  # revealed: A | B
```

### Expanding enums

#### Basic

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
from typing import Literal
from overloaded import SomeEnum, A, B, C, f

def _(x: SomeEnum, y: Literal[SomeEnum.A, SomeEnum.C]):
    reveal_type(f(SomeEnum.A))  # revealed: A
    reveal_type(f(*(SomeEnum.A,)))  # revealed: A

    reveal_type(f(SomeEnum.B))  # revealed: B
    reveal_type(f(*(SomeEnum.B,)))  # revealed: B

    reveal_type(f(SomeEnum.C))  # revealed: C
    reveal_type(f(*(SomeEnum.C,)))  # revealed: C

    reveal_type(f(x))  # revealed: A | B | C
    reveal_type(f(*(x,)))  # revealed: A | B | C

    reveal_type(f(y))  # revealed: A | C
    reveal_type(f(*(y,)))  # revealed: A | C
```

#### Enum with single member

This pattern appears in typeshed. Here, it is used to represent two optional, mutually exclusive
keyword parameters:

`overloaded.pyi`:

```pyi
from enum import Enum, auto
from typing import overload, Literal

class Missing(Enum):
    Value = auto()

class OnlyASpecified: ...
class OnlyBSpecified: ...
class BothMissing: ...

@overload
def f(*, a: int, b: Literal[Missing.Value] = ...) -> OnlyASpecified: ...
@overload
def f(*, a: Literal[Missing.Value] = ..., b: int) -> OnlyBSpecified: ...
@overload
def f(*, a: Literal[Missing.Value] = ..., b: Literal[Missing.Value] = ...) -> BothMissing: ...
```

```py
from typing import Literal
from overloaded import f, Missing

reveal_type(f())  # revealed: BothMissing
reveal_type(f(a=0))  # revealed: OnlyASpecified
reveal_type(f(b=0))  # revealed: OnlyBSpecified

f(a=0, b=0)  # error: [no-matching-overload]

def _(missing: Literal[Missing.Value], missing_or_present: Literal[Missing.Value] | int):
    reveal_type(f(a=missing, b=missing))  # revealed: BothMissing
    reveal_type(f(a=missing))  # revealed: BothMissing
    reveal_type(f(b=missing))  # revealed: BothMissing
    reveal_type(f(a=0, b=missing))  # revealed: OnlyASpecified
    reveal_type(f(a=missing, b=0))  # revealed: OnlyBSpecified

    reveal_type(f(a=missing_or_present))  # revealed: BothMissing | OnlyASpecified
    reveal_type(f(b=missing_or_present))  # revealed: BothMissing | OnlyBSpecified

    # Here, both could be present, so this should be an error
    f(a=missing_or_present, b=missing_or_present)  # error: [no-matching-overload]
```

#### Enum subclass without members

An `Enum` subclass without members should _not_ be expanded:

`overloaded.pyi`:

```pyi
from enum import Enum
from typing import overload, Literal

class MyEnumSubclass(Enum):
    pass

class ActualEnum(MyEnumSubclass):
    A = 1
    B = 2

class OnlyA: ...
class OnlyB: ...
class Both: ...

@overload
def f(x: Literal[ActualEnum.A]) -> OnlyA: ...
@overload
def f(x: Literal[ActualEnum.B]) -> OnlyB: ...
@overload
def f(x: ActualEnum) -> Both: ...
@overload
def f(x: MyEnumSubclass) -> MyEnumSubclass: ...
```

```py
from overloaded import MyEnumSubclass, ActualEnum, f

def _(actual_enum: ActualEnum, my_enum_instance: MyEnumSubclass):
    reveal_type(f(actual_enum))  # revealed: Both
    reveal_type(f(*(actual_enum,)))  # revealed: Both

    reveal_type(f(ActualEnum.A))  # revealed: OnlyA
    reveal_type(f(*(ActualEnum.A,)))  # revealed: OnlyA

    reveal_type(f(ActualEnum.B))  # revealed: OnlyB
    reveal_type(f(*(ActualEnum.B,)))  # revealed: OnlyB

    reveal_type(f(my_enum_instance))  # revealed: MyEnumSubclass
    reveal_type(f(*(my_enum_instance,)))  # revealed: MyEnumSubclass
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
    reveal_type(f(*(ab,)))  # revealed: A | B

    # The `[A | C]` argument list is expanded to `[A], [C]` where the first list matches the first
    # overload while the second list doesn't match any of the overloads, so we generate an
    # error: [no-matching-overload] "No overload of function `f` matches arguments"
    reveal_type(f(ac))  # revealed: Unknown
    # error: [no-matching-overload] "No overload of function `f` matches arguments"
    reveal_type(f(*(ac,)))  # revealed: Unknown

    # None of the expanded argument lists (`[C], [D]`) match any of the overloads, so we generate an
    # error: [no-matching-overload] "No overload of function `f` matches arguments"
    reveal_type(f(cd))  # revealed: Unknown
    # error: [no-matching-overload] "No overload of function `f` matches arguments"
    reveal_type(f(*(cd,)))  # revealed: Unknown
```

### Optimization: Avoid argument type expansion

Argument type expansion could lead to exponential growth of the number of argument lists that needs
to be evaluated, so ty deploys some heuristics to prevent this from happening.

Heuristic: If an argument type that cannot be expanded and cannot be assighned to any of the
remaining overloads before argument type expansion, then even with argument type expansion, it won't
lead to a successful evaluation of the call.

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...
class C: ...

@overload
def f() -> None: ...
@overload
def f(**kwargs: int) -> C: ...
@overload
def f(x: A, /, **kwargs: int) -> A: ...
@overload
def f(x: B, /, **kwargs: int) -> B: ...

class Foo:
    @overload
    def f(self) -> None: ...
    @overload
    def f(self, **kwargs: int) -> C: ...
    @overload
    def f(self, x: A, /, **kwargs: int) -> A: ...
    @overload
    def f(self, x: B, /, **kwargs: int) -> B: ...
```

```py
from overloaded import A, B, C, Foo, f
from typing_extensions import Any, reveal_type

def _(ab: A | B, a: int | Any):
    reveal_type(f(a1=a, a2=a, a3=a))  # revealed: C
    reveal_type(f(A(), a1=a, a2=a, a3=a))  # revealed: A
    reveal_type(f(B(), a1=a, a2=a, a3=a))  # revealed: B

    # Here, the arity check filters out the first and second overload, type checking fails on the
    # remaining overloads, so ty moves on to argument type expansion. But, the first argument (`C`)
    # isn't assignable to any of the remaining overloads (3 and 4), so there's no point in expanding
    # the other 30 arguments of type `Unknown | Literal[1]` which would result in allocating a
    # vector containing 2**30 argument lists after expanding all of the arguments.
    reveal_type(
        # error: [no-matching-overload]
        # revealed: Unknown
        f(
            C(),
            a1=a,
            a2=a,
            a3=a,
            a4=a,
            a5=a,
            a6=a,
            a7=a,
            a8=a,
            a9=a,
            a10=a,
            a11=a,
            a12=a,
            a13=a,
            a14=a,
            a15=a,
            a16=a,
            a17=a,
            a18=a,
            a19=a,
            a20=a,
            a21=a,
            a22=a,
            a23=a,
            a24=a,
            a25=a,
            a26=a,
            a27=a,
            a28=a,
            a29=a,
            a30=a,
        )
    )

    # Here, the heuristics won't come into play because all arguments can be expanded but expanding
    # the first argument resutls in a successful evaluation of the call, so there's no exponential
    # growth of the number of argument lists.
    reveal_type(
        # revealed: A | B
        f(
            ab,
            a1=a,
            a2=a,
            a3=a,
            a4=a,
            a5=a,
            a6=a,
            a7=a,
            a8=a,
            a9=a,
            a10=a,
            a11=a,
            a12=a,
            a13=a,
            a14=a,
            a15=a,
            a16=a,
            a17=a,
            a18=a,
            a19=a,
            a20=a,
            a21=a,
            a22=a,
            a23=a,
            a24=a,
            a25=a,
            a26=a,
            a27=a,
            a28=a,
            a29=a,
            a30=a,
        )
    )

def _(foo: Foo, ab: A | B, a: int | Any):
    reveal_type(foo.f(a1=a, a2=a, a3=a))  # revealed: C
    reveal_type(foo.f(A(), a1=a, a2=a, a3=a))  # revealed: A
    reveal_type(foo.f(B(), a1=a, a2=a, a3=a))  # revealed: B

    reveal_type(
        # error: [no-matching-overload]
        # revealed: Unknown
        foo.f(
            C(),
            a1=a,
            a2=a,
            a3=a,
            a4=a,
            a5=a,
            a6=a,
            a7=a,
            a8=a,
            a9=a,
            a10=a,
            a11=a,
            a12=a,
            a13=a,
            a14=a,
            a15=a,
            a16=a,
            a17=a,
            a18=a,
            a19=a,
            a20=a,
            a21=a,
            a22=a,
            a23=a,
            a24=a,
            a25=a,
            a26=a,
            a27=a,
            a28=a,
            a29=a,
            a30=a,
        )
    )

    reveal_type(
        # revealed: A | B
        foo.f(
            ab,
            a1=a,
            a2=a,
            a3=a,
            a4=a,
            a5=a,
            a6=a,
            a7=a,
            a8=a,
            a9=a,
            a10=a,
            a11=a,
            a12=a,
            a13=a,
            a14=a,
            a15=a,
            a16=a,
            a17=a,
            a18=a,
            a19=a,
            a20=a,
            a21=a,
            a22=a,
            a23=a,
            a24=a,
            a25=a,
            a26=a,
            a27=a,
            a28=a,
            a29=a,
            a30=a,
        )
    )
```

### Optimization: Limit expansion size

<!-- snapshot-diagnostics -->

To prevent combinatorial explosion, ty limits the number of argument lists created by expanding a
single argument.

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...
class C: ...

@overload
def f() -> None: ...
@overload
def f(**kwargs: int) -> C: ...
@overload
def f(x: A, /, **kwargs: int) -> A: ...
@overload
def f(x: B, /, **kwargs: int) -> B: ...
```

```py
from overloaded import A, B, f
from typing_extensions import reveal_type

def _(a: int | None):
    reveal_type(
        # error: [no-matching-overload]
        # revealed: Unknown
        f(
            A(),
            a1=a,
            a2=a,
            a3=a,
            a4=a,
            a5=a,
            a6=a,
            a7=a,
            a8=a,
            a9=a,
            a10=a,
            a11=a,
            a12=a,
            a13=a,
            a14=a,
            a15=a,
            a16=a,
            a17=a,
            a18=a,
            a19=a,
            a20=a,
            a21=a,
            a22=a,
            a23=a,
            a24=a,
            a25=a,
            a26=a,
            a27=a,
            a28=a,
            a29=a,
            a30=a,
        )
    )
```

### Retry from parameter matching

As per the spec, the argument type expansion should retry evaluating the expanded argument list from
the type checking step. However, that creates an issue when variadic arguments are involved because
if a variadic argument is a union type, it could be expanded to have different arities. So, ty
retries it from the start which includes parameter matching as well.

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def f(x: int, y: int) -> None: ...
@overload
def f(x: int, y: str, z: int) -> None: ...
```

```py
from overloaded import f

# Test all of the above with a number of different splatted argument types

def _(t: tuple[int, str]) -> None:
    # This correctly produces an error because the first element of the union has a precise arity of
    # 2, which matches the first overload, but the second element of the tuple doesn't match the
    # second parameter type, yielding an `invalid-argument-type` error.
    f(*t)  # error: [invalid-argument-type]

def _(t: tuple[int, str, int]) -> None:
    # This correctly produces no error because the first element of the union has a precise arity of
    # 3, which matches the second overload.
    f(*t)

def _(t: tuple[int, str] | tuple[int, str, int]) -> None:
    # This produces an error because the expansion produces two argument lists: `[*tuple[int, str]]`
    # and `[*tuple[int, str, int]]`. The first list produces produces a type checking error as
    # described in the first example, while the second list matches the second overload. And,
    # because not all of the expanded argument list evaluates successfully, we produce an error.
    f(*t)  # error: [no-matching-overload]
```

## Filtering based on variadic arguments

This is step 4 of the overload call evaluation algorithm which specifies that:

> If the argument list is compatible with two or more overloads, determine whether one or more of
> the overloads has a variadic parameter (either `*args` or `**kwargs`) that maps to a corresponding
> argument that supplies an indeterminate number of positional or keyword arguments. If so,
> eliminate overloads that do not have a variadic parameter.

This is only performed if the previous step resulted in more than one matching overload.

### Simple `*args`

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def f(x1: int) -> tuple[int]: ...
@overload
def f(x1: int, x2: int) -> tuple[int, int]: ...
@overload
def f(*args: int) -> int: ...
```

```py
from overloaded import f

def _(x1: int, x2: int, args: list[int]):
    reveal_type(f(x1))  # revealed: tuple[int]
    reveal_type(f(x1, x2))  # revealed: tuple[int, int]
    reveal_type(f(*(x1, x2)))  # revealed: tuple[int, int]

    # Step 4 should filter out all but the last overload.
    reveal_type(f(*args))  # revealed: int
```

### Variable `*args`

```toml
[environment]
python-version = "3.11"
```

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def f(x1: int) -> tuple[int]: ...
@overload
def f(x1: int, x2: int) -> tuple[int, int]: ...
@overload
def f(x1: int, *args: int) -> tuple[int, ...]: ...
```

```py
from overloaded import f

def _(x1: int, x2: int, args1: list[int], args2: tuple[int, *tuple[int, ...]]):
    reveal_type(f(x1, x2))  # revealed: tuple[int, int]
    reveal_type(f(*(x1, x2)))  # revealed: tuple[int, int]

    # Step 4 should filter out all but the last overload.
    reveal_type(f(x1, *args1))  # revealed: tuple[int, ...]
    reveal_type(f(*args2))  # revealed: tuple[int, ...]
```

### Simple `**kwargs`

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def f(*, x1: int) -> int: ...
@overload
def f(*, x1: int, x2: int) -> tuple[int, int]: ...
@overload
def f(**kwargs: int) -> int: ...
```

```py
from overloaded import f

def _(x1: int, x2: int, kwargs: dict[str, int]):
    reveal_type(f(x1=x1))  # revealed: int
    reveal_type(f(x1=x1, x2=x2))  # revealed: tuple[int, int]

    # Step 4 should filter out all but the last overload.
    reveal_type(f(**{"x1": x1, "x2": x2}))  # revealed: int
    reveal_type(f(**kwargs))  # revealed: int
```

### `TypedDict`

The keys in a `TypedDict` are static so there's no variable part to it, so step 4 shouldn't filter
out any overloads.

`overloaded.pyi`:

```pyi
from typing import TypedDict, overload

@overload
def f(*, x: int) -> int: ...
@overload
def f(*, x: int, y: int) -> tuple[int, int]: ...
@overload
def f(**kwargs: int) -> tuple[int, ...]: ...
```

```py
from typing import TypedDict
from overloaded import f

class Foo(TypedDict):
    x: int
    y: int

def _(foo: Foo, kwargs: dict[str, int]):
    reveal_type(f(**foo))  # revealed: tuple[int, int]
    reveal_type(f(**kwargs))  # revealed: tuple[int, ...]
```

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
reveal_type(f(*(1,)))  # revealed: str

def _(list_int: list[int], list_any: list[Any]):
    reveal_type(f(list_int))  # revealed: int
    reveal_type(f(*(list_int,)))  # revealed: int

    reveal_type(f(list_any))  # revealed: int
    reveal_type(f(*(list_any,)))  # revealed: int
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
reveal_type(f(*(1,)))  # revealed: str

def _(list_int: list[int], list_any: list[Any]):
    # All materializations of `list[int]` are assignable to `list[int]`, so it matches the first
    # overload.
    reveal_type(f(list_int))  # revealed: int
    reveal_type(f(*(list_int,)))  # revealed: int

    # All materializations of `list[Any]` are assignable to `list[int]` and `list[Any]`, but the
    # return type of first and second overloads are not equivalent, so the overload matching
    # is ambiguous.
    reveal_type(f(list_any))  # revealed: Unknown
    reveal_type(f(*(list_any,)))  # revealed: Unknown
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
reveal_type(f(*("a",)))  # revealed: str

reveal_type(f((1, "b")))  # revealed: int
reveal_type(f(*((1, "b"),)))  # revealed: int

reveal_type(f((1, 2)))  # revealed: int
reveal_type(f(*((1, 2),)))  # revealed: int

def _(int_str: tuple[int, str], int_any: tuple[int, Any], any_any: tuple[Any, Any]):
    # All materializations are assignable to first overload, so second and third overloads are
    # eliminated
    reveal_type(f(int_str))  # revealed: int
    reveal_type(f(*(int_str,)))  # revealed: int

    # All materializations are assignable to second overload, so the third overload is eliminated;
    # the return type of first and second overload is equivalent
    reveal_type(f(int_any))  # revealed: int
    reveal_type(f(*(int_any,)))  # revealed: int

    # All materializations of `tuple[Any, Any]` are assignable to the parameters of all the
    # overloads, but the return types aren't equivalent, so the overload matching is ambiguous
    reveal_type(f(any_any))  # revealed: Unknown
    reveal_type(f(*(any_any,)))  # revealed: Unknown
```

### `Unknown` passed into an overloaded function annotated with protocols

`Foo.join()` here has similar annotations to `str.join()` in typeshed:

`module.pyi`:

```pyi
from typing_extensions import Iterable, overload, LiteralString, Protocol
from ty_extensions import Unknown, is_assignable_to

class Foo:
    @overload
    def join(self, iterable: Iterable[LiteralString], /) -> LiteralString: ...
    @overload
    def join(self, iterable: Iterable[str], /) -> str: ...
```

`main.py`:

```py
from module import Foo
from typing_extensions import LiteralString

def f(a: Foo, b: list[str], c: list[LiteralString], e):
    reveal_type(e)  # revealed: Unknown
    reveal_type(a.join(b))  # revealed: str
    reveal_type(a.join(c))  # revealed: LiteralString

    # since both overloads match and they have return types that are not equivalent,
    # step (5) of the overload evaluation algorithm says we must evaluate the result of the
    # call as `Unknown`.
    #
    # Note: although the spec does not state as such (since intersections in general are not
    # specified currently), `(str | LiteralString) & Unknown` might also be a reasonable type
    # here (the union of all overload returns, intersected with `Unknown`) -- here that would
    # simplify to `str & Unknown`.
    reveal_type(a.join(e))  # revealed: Unknown
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
    reveal_type(f(*(list_int, int_str)))  # revealed: A

    # All materialization of first argument is assignable to first overload and for the second
    # argument, they're assignable to the second overload, so the third overload is filtered out
    reveal_type(f(list_int, int_any))  # revealed: A
    reveal_type(f(*(list_int, int_any)))  # revealed: A

    # All materialization of first argument is assignable to second overload and for the second
    # argument, they're assignable to the first overload, so the third overload is filtered out
    reveal_type(f(list_any, int_str))  # revealed: A
    reveal_type(f(*(list_any, int_str)))  # revealed: A

    # All materializations of both arguments are assignable to the second overload, so the third
    # overload is filtered out
    reveal_type(f(list_any, int_any))  # revealed: A
    reveal_type(f(*(list_any, int_any)))  # revealed: A

    # All materializations of first argument is assignable to the second overload and for the second
    # argument, they're assignable to the third overload, so no overloads are filtered out; the
    # return types of the remaining overloads are not equivalent, so overload matching is ambiguous
    reveal_type(f(list_int, any_any))  # revealed: Unknown
    reveal_type(f(*(list_int, any_any)))  # revealed: Unknown
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
    reveal_type(f(*(literal,)))  # revealed: LiteralString

    reveal_type(f(string))  # revealed: str
    reveal_type(f(*(string,)))  # revealed: str

    # `Any` matches both overloads, but the return types are not equivalent.
    # Pyright and mypy both reveal `str` here, contrary to the spec.
    reveal_type(f(any))  # revealed: Unknown
    reveal_type(f(*(any,)))  # revealed: Unknown
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
    reveal_type(f(*(list_int,)))  # revealed: A

    reveal_type(f(list_str))  # revealed: str
    reveal_type(f(*(list_str,)))  # revealed: str

    reveal_type(f(list_any))  # revealed: Unknown
    reveal_type(f(*(list_any,)))  # revealed: Unknown

    reveal_type(f(any))  # revealed: Unknown
    reveal_type(f(*(any,)))  # revealed: Unknown
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
    reveal_type(f(*(integer, string)))  # revealed: int

    reveal_type(f(string, integer))  # revealed: int
    reveal_type(f(*(string, integer)))  # revealed: int

    # This matches the second overload and is _not_ the case of ambiguous overload matching.
    reveal_type(f(string, any))  # revealed: Any
    reveal_type(f(*(string, any)))  # revealed: Any

    reveal_type(f(string, list_any))  # revealed: list[Any]
    reveal_type(f(*(string, list_any)))  # revealed: list[Any]
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

`overloaded.pyi`:

```pyi
from typing import Any, overload

class A: ...
class B: ...

@overload
def f1(x: int) -> A: ...
@overload
def f1(x: Any, y: Any) -> A: ...

@overload
def f2(x: int) -> A: ...
@overload
def f2(x: Any, y: Any) -> B: ...

@overload
def f3(x: int) -> A: ...
@overload
def f3(x: Any, y: Any) -> A: ...
@overload
def f3(x: Any, y: Any, *, z: str) -> B: ...

@overload
def f4(x: int) -> A: ...
@overload
def f4(x: Any, y: Any) -> B: ...
@overload
def f4(x: Any, y: Any, *, z: str) -> B: ...
```

```py
from typing import Any

from overloaded import f1, f2, f3, f4

def _(arg: list[Any]):
    # Matches both overload and the return types are equivalent
    reveal_type(f1(*arg))  # revealed: A
    # Matches both overload but the return types aren't equivalent
    reveal_type(f2(*arg))  # revealed: Unknown
    # Filters out the final overload and the return types are equivalent
    reveal_type(f3(*arg))  # revealed: A
    # Filters out the final overload but the return types aren't equivalent
    reveal_type(f4(*arg))  # revealed: Unknown
```

### Varidic argument with generics

`overloaded.pyi`:

```pyi
from typing import Any, TypeVar, overload

T1 = TypeVar("T1")
T2 = TypeVar("T2")
T3 = TypeVar("T3")

@overload
def f1(x: T1, /) -> tuple[T1]: ...
@overload
def f1(x1: T1, x2: T2, /) -> tuple[T1, T2]: ...
@overload
def f1(x1: T1, x2: T2, x3: T3, /) -> tuple[T1, T2, T3]: ...
@overload
def f1(*args: Any) -> tuple[Any, ...]: ...

@overload
def f2(x1: T1) -> tuple[T1]: ...
@overload
def f2(x1: T1, x2: T2) -> tuple[T1, T2]: ...
@overload
def f2(*args: Any, **kwargs: Any) -> tuple[Any, ...]: ...

@overload
def f3(x: T1) -> tuple[T1]: ...
@overload
def f3(x1: T1, x2: T2) -> tuple[T1, T2]: ...
@overload
def f3(*args: Any) -> tuple[Any, ...]: ...
@overload
def f3(**kwargs: Any) -> dict[str, Any]: ...
```

```py
from overloaded import f1, f2, f3
from typing import Any

# These calls only match the last overload
reveal_type(f1())  # revealed: tuple[Any, ...]
reveal_type(f1(1, 2, 3, 4))  # revealed: tuple[Any, ...]

# While these calls match multiple overloads but step 5 filters out all the remaining overloads
# except the most specific one in terms of the number of arguments.
reveal_type(f1(1))  # revealed: tuple[Literal[1]]
reveal_type(f1(1, 2))  # revealed: tuple[Literal[1], Literal[2]]
reveal_type(f1(1, 2, 3))  # revealed: tuple[Literal[1], Literal[2], Literal[3]]

def _(args1: list[int], args2: list[Any]):
    reveal_type(f1(*args1))  # revealed: tuple[Any, ...]
    reveal_type(f1(*args2))  # revealed: tuple[Any, ...]

reveal_type(f2())  # revealed: tuple[Any, ...]
reveal_type(f2(1, 2))  # revealed: tuple[Literal[1], Literal[2]]
# TODO: Should be `tuple[Literal[1], Literal[2]]`
reveal_type(f2(x1=1, x2=2))  # revealed: Unknown
# TODO: Should be `tuple[Literal[2], Literal[1]]`
reveal_type(f2(x2=1, x1=2))  # revealed: Unknown
reveal_type(f2(1, 2, z=3))  # revealed: tuple[Any, ...]

reveal_type(f3(1, 2))  # revealed: tuple[Literal[1], Literal[2]]
reveal_type(f3(1, 2, 3))  # revealed: tuple[Any, ...]
# TODO: Should be `tuple[Literal[1], Literal[2]]`
reveal_type(f3(x1=1, x2=2))  # revealed: Unknown
reveal_type(f3(z=1))  # revealed: dict[str, Any]

# error: [no-matching-overload]
reveal_type(f3(1, 2, x=3))  # revealed: Unknown
```

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
    reveal_type(f(*(any,), flag=True))  # revealed: int

    reveal_type(f(any, flag=False))  # revealed: str
    reveal_type(f(*(any,), flag=False))  # revealed: str
```

### Non-participating gradual parameter

`overloaded.pyi`:

```pyi
from typing import Any, Literal, overload

@overload
def f(x: tuple[str, Any], flag: Literal[True]) -> int: ...
@overload
def f(x: tuple[str, Any], flag: Literal[False] = ...) -> str: ...
@overload
def f(x: tuple[str, Any], flag: bool = ...) -> int | str: ...
```

```py
from typing import Any, Literal

from overloaded import f

def _(any: Any):
    reveal_type(f(any, flag=True))  # revealed: int
    reveal_type(f(*(any,), flag=True))  # revealed: int

    reveal_type(f(any, flag=False))  # revealed: str
    reveal_type(f(*(any,), flag=False))  # revealed: str

def _(args: tuple[Any, Literal[True]]):
    reveal_type(f(*args))  # revealed: int

def _(args: tuple[Any, Literal[False]]):
    reveal_type(f(*args))  # revealed: str
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
    reveal_type(f(*(arg,)))  # revealed: A | B
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
    reveal_type(f(*(arg,)))  # revealed: A | Unknown
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
    reveal_type(f(*(arg,)))  # revealed: Unknown
```

## Bidirectional Type Inference

```toml
[environment]
python-version = "3.12"
```

Type inference accounts for parameter type annotations across all overloads.

```py
from typing import TypedDict, overload

class T(TypedDict):
    x: int

@overload
def f(a: list[T], b: int) -> int: ...
@overload
def f(a: list[dict[str, int]], b: str) -> str: ...
def f(a: list[dict[str, int]] | list[T], b: int | str) -> int | str:
    return 1

def int_or_str() -> int | str:
    return 1

x = f([{"x": 1}], int_or_str())
reveal_type(x)  # revealed: int | str

# error: [no-matching-overload] "No overload of function `f` matches arguments"
f([{"y": 1}], int_or_str())
```

Non-matching overloads do not produce diagnostics:

```py
from typing import TypedDict, overload

class T(TypedDict):
    x: int

@overload
def f(a: T, b: int) -> int: ...
@overload
def f(a: dict[str, int], b: str) -> str: ...
def f(a: T | dict[str, int], b: int | str) -> int | str:
    return 1

x = f({"y": 1}, "a")
reveal_type(x)  # revealed: str
```

```py
from typing import SupportsRound, overload

@overload
def takes_str_or_float(x: str): ...
@overload
def takes_str_or_float(x: float): ...
def takes_str_or_float(x: float | str): ...

takes_str_or_float(round(1.0))
```
