# Equivalence relation

`is_equivalent_to` implements [the equivalence relation] on types.

For fully static types, two types `A` and `B` are equivalent iff `A` is a subtype of `B` and `B` is
a subtype of `A` (that is, the two types represent the same set of values).

Two gradual types `A` and `B` are equivalent if all [materializations] of `A` are also
materializations of `B`, and all materializations of `B` are also materializations of `A`.

## Basic

### Fully static

```py
from typing_extensions import Literal, LiteralString, Protocol, Never
from ty_extensions import Unknown, is_equivalent_to, static_assert, TypeOf, AlwaysTruthy, AlwaysFalsy
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

class Single(Enum):
    VALUE = 1

static_assert(is_equivalent_to(Literal[1, 2], Literal[1, 2]))
static_assert(is_equivalent_to(type[object], type))
static_assert(is_equivalent_to(type, type[object]))

static_assert(not is_equivalent_to(Literal[1, 2], Literal[1, 0]))
static_assert(not is_equivalent_to(Literal[1, 0], Literal[1, 2]))
static_assert(not is_equivalent_to(Literal[1, 2], Literal[1, 2, 3]))
static_assert(not is_equivalent_to(Literal[1, 2, 3], Literal[1, 2]))

static_assert(is_equivalent_to(Literal[Answer.YES], Literal[Answer.YES]))
static_assert(is_equivalent_to(Literal[Answer.NO, Answer.YES], Answer))
static_assert(is_equivalent_to(Literal[Answer.YES, Answer.NO], Answer))
static_assert(not is_equivalent_to(Literal[Answer.YES], Literal[Answer.NO]))
static_assert(not is_equivalent_to(Literal[Answer.YES], Answer))

static_assert(is_equivalent_to(Literal[Single.VALUE], Single))
static_assert(is_equivalent_to(Single, Literal[Single.VALUE]))
static_assert(is_equivalent_to(Literal[Single.VALUE], Literal[Single.VALUE]))

static_assert(is_equivalent_to(tuple[Single] | int | str, str | int | tuple[Literal[Single.VALUE]]))

class Protocol1(Protocol):
    a: Single

class Protocol2(Protocol):
    a: Literal[Single.VALUE]

static_assert(is_equivalent_to(Protocol1, Protocol2))

static_assert(is_equivalent_to(Never, Never))
static_assert(is_equivalent_to(AlwaysTruthy, AlwaysTruthy))
static_assert(is_equivalent_to(AlwaysFalsy, AlwaysFalsy))
static_assert(is_equivalent_to(LiteralString, LiteralString))

static_assert(is_equivalent_to(Literal[True], Literal[True]))
static_assert(is_equivalent_to(Literal[False], Literal[False]))
static_assert(is_equivalent_to(TypeOf[0:1:2], TypeOf[0:1:2]))

static_assert(is_equivalent_to(TypeOf[str], TypeOf[str]))
static_assert(is_equivalent_to(type, type[object]))
```

### Gradual

```py
from typing import Any
from typing_extensions import Literal, LiteralString, Never
from ty_extensions import Unknown, is_equivalent_to, static_assert

static_assert(is_equivalent_to(Any, Any))
static_assert(is_equivalent_to(Unknown, Unknown))
static_assert(is_equivalent_to(Any, Unknown))
static_assert(not is_equivalent_to(Any, None))

static_assert(not is_equivalent_to(type, type[Any]))
static_assert(not is_equivalent_to(type[object], type[Any]))
```

## Unions and intersections

```py
from typing import Any, Literal
from ty_extensions import Intersection, Not, Unknown, is_equivalent_to, static_assert
from enum import Enum

static_assert(is_equivalent_to(str | int, str | int))
static_assert(is_equivalent_to(str | int | Any, str | int | Unknown))
static_assert(is_equivalent_to(str | int, int | str))
static_assert(is_equivalent_to(Intersection[str, int, Not[bytes], Not[None]], Intersection[int, str, Not[None], Not[bytes]]))
static_assert(is_equivalent_to(Intersection[str | int, Not[type[Any]]], Intersection[int | str, Not[type[Unknown]]]))

static_assert(not is_equivalent_to(str | int, int | str | bytes))
static_assert(not is_equivalent_to(str | int | bytes, int | str | dict))

static_assert(is_equivalent_to(Unknown, Unknown | Any))
static_assert(is_equivalent_to(Unknown, Intersection[Unknown, Any]))

class P: ...
class Q: ...
class R: ...
class S: ...

static_assert(is_equivalent_to(P | Q | R, P | R | Q))  # 1
static_assert(is_equivalent_to(P | Q | R, Q | P | R))  # 2
static_assert(is_equivalent_to(P | Q | R, Q | R | P))  # 3
static_assert(is_equivalent_to(P | Q | R, R | P | Q))  # 4
static_assert(is_equivalent_to(P | Q | R, R | Q | P))  # 5
static_assert(is_equivalent_to(P | R | Q, Q | P | R))  # 6
static_assert(is_equivalent_to(P | R | Q, Q | R | P))  # 7
static_assert(is_equivalent_to(P | R | Q, R | P | Q))  # 8
static_assert(is_equivalent_to(P | R | Q, R | Q | P))  # 9
static_assert(is_equivalent_to(Q | P | R, Q | R | P))  # 10
static_assert(is_equivalent_to(Q | P | R, R | P | Q))  # 11
static_assert(is_equivalent_to(Q | P | R, R | Q | P))  # 12
static_assert(is_equivalent_to(Q | R | P, R | P | Q))  # 13
static_assert(is_equivalent_to(Q | R | P, R | Q | P))  # 14
static_assert(is_equivalent_to(R | P | Q, R | Q | P))  # 15

static_assert(is_equivalent_to(str | None, None | str))

static_assert(is_equivalent_to(Intersection[P, Q], Intersection[Q, P]))
static_assert(is_equivalent_to(Intersection[Q, Not[P]], Intersection[Not[P], Q]))
static_assert(is_equivalent_to(Intersection[Q, R, Not[P]], Intersection[Not[P], R, Q]))
static_assert(is_equivalent_to(Intersection[Q | R, Not[P | S]], Intersection[Not[S | P], R | Q]))

class Single(Enum):
    VALUE = 1

static_assert(is_equivalent_to(P | Q | Single, Literal[Single.VALUE] | Q | P))

static_assert(is_equivalent_to(Any, Any | Intersection[Any, str]))
static_assert(is_equivalent_to(Any, Intersection[str, Any] | Any))
static_assert(is_equivalent_to(Any, Any | Intersection[Any, Not[None]]))
static_assert(is_equivalent_to(Any, Intersection[Not[None], Any] | Any))

static_assert(is_equivalent_to(Any, Unknown | Intersection[Unknown, str]))
static_assert(is_equivalent_to(Any, Intersection[str, Unknown] | Unknown))
static_assert(is_equivalent_to(Any, Unknown | Intersection[Unknown, Not[None]]))
static_assert(is_equivalent_to(Any, Intersection[Not[None], Unknown] | Unknown))
```

## Tuples

```py
from ty_extensions import Unknown, is_equivalent_to, static_assert
from typing import Any

static_assert(is_equivalent_to(tuple[str, Any], tuple[str, Unknown]))

static_assert(not is_equivalent_to(tuple[str, int], tuple[str, int, bytes]))
static_assert(not is_equivalent_to(tuple[str, int], tuple[int, str]))
```

## Tuples containing equivalent but differently ordered unions/intersections are equivalent

```py
from ty_extensions import is_equivalent_to, TypeOf, static_assert, Intersection, Not
from typing import Literal

class P: ...
class Q: ...
class R: ...
class S: ...

static_assert(is_equivalent_to(tuple[P | Q], tuple[Q | P]))
static_assert(is_equivalent_to(tuple[P | None], tuple[None | P]))
static_assert(
    is_equivalent_to(tuple[Intersection[P, Q] | Intersection[R, Not[S]]], tuple[Intersection[Not[S], R] | Intersection[Q, P]])
)
```

## Unions containing tuples containing tuples containing unions (etc.)

```py
from ty_extensions import is_equivalent_to, static_assert, Intersection

class P: ...
class Q: ...

static_assert(
    is_equivalent_to(
        tuple[tuple[tuple[P | Q]]] | P,
        tuple[tuple[tuple[Q | P]]] | P,
    )
)
static_assert(
    is_equivalent_to(
        tuple[tuple[tuple[tuple[tuple[Intersection[P, Q]]]]]],
        tuple[tuple[tuple[tuple[tuple[Intersection[Q, P]]]]]],
    )
)
```

## Intersections containing tuples containing unions

```py
from ty_extensions import is_equivalent_to, static_assert, Intersection

class P: ...
class Q: ...
class R: ...

static_assert(is_equivalent_to(Intersection[tuple[P | Q], R], Intersection[tuple[Q | P], R]))
```

## Unions containing generic instances parameterized by unions

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import is_equivalent_to, static_assert

class A: ...
class B: ...
class Foo[T]: ...

static_assert(is_equivalent_to(A | Foo[A | B], Foo[B | A] | A))
```

## Callable

### Equivalent

For an equivalence relationship, the default value does not necessarily need to be the same but if
the parameter in one of the callable has a default value then the corresponding parameter in the
other callable should also have a default value.

```py
from ty_extensions import CallableTypeOf, is_equivalent_to, static_assert
from typing import Callable

def f1(a: int = 1) -> None: ...
def f2(a: int = 2) -> None: ...

static_assert(is_equivalent_to(CallableTypeOf[f1], CallableTypeOf[f2]))
static_assert(is_equivalent_to(CallableTypeOf[f1] | bool | CallableTypeOf[f2], CallableTypeOf[f2] | bool | CallableTypeOf[f1]))
```

The names of the positional-only, variadic and keyword-variadic parameters does not need to be the
same.

```py
def f3(a1: int, /, *args1: int, **kwargs2: int) -> None: ...
def f4(a2: int, /, *args2: int, **kwargs1: int) -> None: ...

static_assert(is_equivalent_to(CallableTypeOf[f3], CallableTypeOf[f4]))
static_assert(is_equivalent_to(CallableTypeOf[f3] | bool | CallableTypeOf[f4], CallableTypeOf[f4] | bool | CallableTypeOf[f3]))
```

Putting it all together, the following two callables are equivalent:

```py
def f5(a1: int, /, b: float, c: bool = False, *args1: int, d: int = 1, e: str, **kwargs1: float) -> None: ...
def f6(a2: int, /, b: float, c: bool = True, *args2: int, d: int = 2, e: str, **kwargs2: float) -> None: ...

static_assert(is_equivalent_to(CallableTypeOf[f5], CallableTypeOf[f6]))
static_assert(is_equivalent_to(CallableTypeOf[f5] | bool | CallableTypeOf[f6], CallableTypeOf[f6] | bool | CallableTypeOf[f5]))
```

### Not equivalent

There are multiple cases when two callable types are not equivalent which are enumerated below.

```py
from ty_extensions import CallableTypeOf, is_equivalent_to, static_assert
from typing import Callable
```

When the number of parameters is different:

```py
def f1(a: int) -> None: ...
def f2(a: int, b: int) -> None: ...

static_assert(not is_equivalent_to(CallableTypeOf[f1], CallableTypeOf[f2]))
```

When the return types are not equivalent in one or both of the callable types:

```py
def f3(): ...
def f4() -> None: ...

static_assert(not is_equivalent_to(Callable[[], int], Callable[[], None]))
static_assert(is_equivalent_to(CallableTypeOf[f3], CallableTypeOf[f3]))
static_assert(not is_equivalent_to(CallableTypeOf[f3], CallableTypeOf[f4]))
static_assert(not is_equivalent_to(CallableTypeOf[f4], CallableTypeOf[f3]))
```

When the parameter names are different:

```py
def f5(a: int) -> None: ...
def f6(b: int) -> None: ...

static_assert(not is_equivalent_to(CallableTypeOf[f5], CallableTypeOf[f6]))
```

When only one of the callable types has parameter names:

```py
static_assert(not is_equivalent_to(CallableTypeOf[f5], Callable[[int], None]))
```

When the parameter kinds are different:

```py
def f7(a: int, /) -> None: ...
def f8(a: int) -> None: ...

static_assert(not is_equivalent_to(CallableTypeOf[f7], CallableTypeOf[f8]))
```

When the annotated types of the parameters are not equivalent or absent in one or both of the
callable types:

```py
def f9(a: int) -> None: ...
def f10(a: str) -> None: ...
def f11(a) -> None: ...

static_assert(not is_equivalent_to(CallableTypeOf[f9], CallableTypeOf[f10]))
static_assert(not is_equivalent_to(CallableTypeOf[f10], CallableTypeOf[f11]))
static_assert(not is_equivalent_to(CallableTypeOf[f11], CallableTypeOf[f10]))
static_assert(is_equivalent_to(CallableTypeOf[f11], CallableTypeOf[f11]))
```

When the default value for a parameter is present only in one of the callable type:

```py
def f12(a: int) -> None: ...
def f13(a: int = 2) -> None: ...

static_assert(not is_equivalent_to(CallableTypeOf[f12], CallableTypeOf[f13]))
static_assert(not is_equivalent_to(CallableTypeOf[f13], CallableTypeOf[f12]))
```

### Unions containing `Callable`s

Two unions containing different `Callable` types are equivalent even if the unions are differently
ordered:

```py
from ty_extensions import CallableTypeOf, Unknown, is_equivalent_to, static_assert

def f(x): ...
def g(x: Unknown): ...

static_assert(is_equivalent_to(CallableTypeOf[f] | int | str, str | int | CallableTypeOf[g]))
```

### Unions containing `Callable`s containing unions

Differently ordered unions inside `Callable`s inside unions can still be equivalent:

```py
from typing import Callable
from ty_extensions import is_equivalent_to, static_assert

static_assert(is_equivalent_to(int | Callable[[int | str], None], Callable[[str | int], None] | int))
```

### Overloads

#### One overload

`overloaded.pyi`:

```pyi
from typing import overload

class Grandparent: ...
class Parent(Grandparent): ...
class Child(Parent): ...

@overload
def overloaded(a: Child) -> None: ...
@overload
def overloaded(a: Parent) -> None: ...
@overload
def overloaded(a: Grandparent) -> None: ...
```

```py
from ty_extensions import CallableTypeOf, is_equivalent_to, static_assert
from overloaded import Grandparent, Parent, Child, overloaded

def grandparent(a: Grandparent) -> None: ...

static_assert(is_equivalent_to(CallableTypeOf[grandparent], CallableTypeOf[overloaded]))
static_assert(is_equivalent_to(CallableTypeOf[overloaded], CallableTypeOf[grandparent]))
```

#### Both overloads

`overloaded.pyi`:

```pyi
from typing import overload

class Grandparent: ...
class Parent(Grandparent): ...
class Child(Parent): ...

@overload
def pg(a: Parent) -> None: ...
@overload
def pg(a: Grandparent) -> None: ...

@overload
def cpg(a: Child) -> None: ...
@overload
def cpg(a: Parent) -> None: ...
@overload
def cpg(a: Grandparent) -> None: ...
```

```py
from ty_extensions import CallableTypeOf, is_equivalent_to, static_assert
from overloaded import pg, cpg

static_assert(is_equivalent_to(CallableTypeOf[pg], CallableTypeOf[cpg]))
static_assert(is_equivalent_to(CallableTypeOf[cpg], CallableTypeOf[pg]))
```

### Function-literal types and bound-method types

Function-literal types and bound-method types are always considered self-equivalent.

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import is_equivalent_to, TypeOf, static_assert

def f(): ...

static_assert(is_equivalent_to(TypeOf[f], TypeOf[f]))

class A:
    def method(self) -> int:
        return 42

static_assert(is_equivalent_to(TypeOf[A.method], TypeOf[A.method]))
type X = TypeOf[A.method]
static_assert(is_equivalent_to(X, X))
```

### Non-fully-static callable types

The examples provided below are only a subset of the possible cases and only include the ones with
gradual types. The cases with fully static types and using different combinations of parameter kinds
are covered above.

```py
from ty_extensions import Unknown, CallableTypeOf, TypeOf, is_equivalent_to, static_assert
from typing import Any, Callable

static_assert(is_equivalent_to(Callable[..., int], Callable[..., int]))
static_assert(is_equivalent_to(Callable[..., Any], Callable[..., Unknown]))
static_assert(is_equivalent_to(Callable[[int, Any], None], Callable[[int, Unknown], None]))

static_assert(not is_equivalent_to(Callable[[int, Any], None], Callable[[Any, int], None]))
static_assert(not is_equivalent_to(Callable[[int, str], None], Callable[[int, str, bytes], None]))
static_assert(not is_equivalent_to(Callable[..., None], Callable[[], None]))
```

A function with no explicit return type should be gradually equivalent to a function-like callable
with a return type of `Any`.

```py
def f1():
    return

def f1_equivalent() -> Any:
    return

static_assert(is_equivalent_to(CallableTypeOf[f1], CallableTypeOf[f1_equivalent]))
```

And, similarly for parameters with no annotations.

```py
def f2(a, b, /) -> None:
    return

def f2_equivalent(a: Any, b: Any, /) -> None:
    return

static_assert(is_equivalent_to(CallableTypeOf[f2], CallableTypeOf[f2_equivalent]))
```

A function definition that includes both `*args` and `**kwargs` parameter that are annotated as
`Any` or kept unannotated should be gradual equivalent to a callable with `...` as the parameter
type.

```py
def variadic_without_annotation(*args, **kwargs):
    return

def variadic_with_annotation(*args: Any, **kwargs: Any) -> Any:
    return

def _(
    signature_variadic_without_annotation: CallableTypeOf[variadic_without_annotation],
    signature_variadic_with_annotation: CallableTypeOf[variadic_with_annotation],
) -> None:
    # revealed: (...) -> Unknown
    reveal_type(signature_variadic_without_annotation)
    # revealed: (...) -> Any
    reveal_type(signature_variadic_with_annotation)
```

Note that `variadic_without_annotation` and `variadic_with_annotation` are *not* considered
gradually equivalent to `Callable[..., Any]`, because the latter is not a function-like callable
type:

```py
static_assert(not is_equivalent_to(CallableTypeOf[variadic_without_annotation], Callable[..., Any]))
static_assert(not is_equivalent_to(CallableTypeOf[variadic_with_annotation], Callable[..., Any]))
```

A function with either `*args` or `**kwargs` (and not both) is is not equivalent to a callable with
`...` as the parameter type.

```py
def variadic_args(*args):
    return

def variadic_kwargs(**kwargs):
    return

def _(
    signature_variadic_args: CallableTypeOf[variadic_args],
    signature_variadic_kwargs: CallableTypeOf[variadic_kwargs],
) -> None:
    # revealed: (*args) -> Unknown
    reveal_type(signature_variadic_args)
    # revealed: (**kwargs) -> Unknown
    reveal_type(signature_variadic_kwargs)

static_assert(not is_equivalent_to(CallableTypeOf[variadic_args], Callable[..., Any]))
static_assert(not is_equivalent_to(CallableTypeOf[variadic_kwargs], Callable[..., Any]))
```

Parameter names, default values, and it's kind should also be considered when checking for gradual
equivalence.

```py
def f1(a): ...
def f2(b): ...

static_assert(not is_equivalent_to(CallableTypeOf[f1], CallableTypeOf[f2]))

def f3(a=1): ...
def f4(a=2): ...
def f5(a): ...

static_assert(is_equivalent_to(CallableTypeOf[f3], CallableTypeOf[f4]))
static_assert(is_equivalent_to(CallableTypeOf[f3] | bool | CallableTypeOf[f4], CallableTypeOf[f4] | bool | CallableTypeOf[f3]))
static_assert(not is_equivalent_to(CallableTypeOf[f3], CallableTypeOf[f5]))

def f6(a, /): ...

static_assert(not is_equivalent_to(CallableTypeOf[f1], CallableTypeOf[f6]))
```

## Module-literal types

Two "copies" of a single-file module are considered equivalent types, even if the different copies
were originally imported in different first-party modules:

`module.py`:

```py
import typing
```

`main.py`:

```py
import typing
from module import typing as other_typing
from ty_extensions import TypeOf, static_assert, is_equivalent_to

static_assert(is_equivalent_to(TypeOf[typing], TypeOf[other_typing]))
static_assert(is_equivalent_to(TypeOf[typing] | int | str, str | int | TypeOf[other_typing]))
```

We currently do not consider module-literal types to be equivalent if the underlying module is a
package and the different "copies" of the module were originally imported in different modules. This
is because we might consider submodules to be available as attributes on one copy but not on the
other, depending on whether those submodules were explicitly imported in the original importing
module:

`module2.py`:

```py
import imported
import imported.abc
```

`imported/__init__.pyi`:

```pyi
```

`imported/abc.pyi`:

```pyi
```

`main2.py`:

```py
import imported
from module2 import imported as other_imported
from ty_extensions import TypeOf, static_assert, is_equivalent_to

# error: [possibly-missing-attribute]
reveal_type(imported.abc)  # revealed: Unknown

reveal_type(other_imported.abc)  # revealed: <module 'imported.abc'>

static_assert(not is_equivalent_to(TypeOf[imported], TypeOf[other_imported]))
```

[materializations]: https://typing.python.org/en/latest/spec/glossary.html#term-materialize
[the equivalence relation]: https://typing.python.org/en/latest/spec/glossary.html#term-equivalent
