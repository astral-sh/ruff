# Equivalence relation

`is_equivalent_to` implements [the equivalence relation] for fully static types.

Two types `A` and `B` are equivalent iff `A` is a subtype of `B` and `B` is a subtype of `A`.

## Basic

```py
from typing import Any
from typing_extensions import Literal
from ty_extensions import Unknown, is_equivalent_to, static_assert

static_assert(is_equivalent_to(Literal[1, 2], Literal[1, 2]))
static_assert(is_equivalent_to(type[object], type))

static_assert(not is_equivalent_to(Any, Any))
static_assert(not is_equivalent_to(Unknown, Unknown))
static_assert(not is_equivalent_to(Any, None))
static_assert(not is_equivalent_to(Literal[1, 2], Literal[1, 0]))
static_assert(not is_equivalent_to(Literal[1, 2], Literal[1, 2, 3]))
```

## Equivalence is commutative

```py
from typing_extensions import Literal
from ty_extensions import is_equivalent_to, static_assert

static_assert(is_equivalent_to(type, type[object]))
static_assert(not is_equivalent_to(Literal[1, 0], Literal[1, 2]))
static_assert(not is_equivalent_to(Literal[1, 2, 3], Literal[1, 2]))
```

## Differently ordered intersections and unions are equivalent

```py
from ty_extensions import is_equivalent_to, static_assert, Intersection, Not

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

When either of the callable types uses a gradual form for the parameters:

```py
static_assert(not is_equivalent_to(Callable[..., None], Callable[[int], None]))
static_assert(not is_equivalent_to(Callable[[int], None], Callable[..., None]))
```

When the return types are not equivalent or absent in one or both of the callable types:

```py
def f3(): ...
def f4() -> None: ...

static_assert(not is_equivalent_to(Callable[[], int], Callable[[], None]))
static_assert(not is_equivalent_to(CallableTypeOf[f3], CallableTypeOf[f3]))
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
static_assert(not is_equivalent_to(CallableTypeOf[f11], CallableTypeOf[f11]))
```

When the default value for a parameter is present only in one of the callable type:

```py
def f12(a: int) -> None: ...
def f13(a: int = 2) -> None: ...

static_assert(not is_equivalent_to(CallableTypeOf[f12], CallableTypeOf[f13]))
static_assert(not is_equivalent_to(CallableTypeOf[f13], CallableTypeOf[f12]))
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

## Function-literal types and bound-method types

Function-literal types and bound-method types are always considered self-equivalent, even if they
have unannotated parameters, or parameters with not-fully-static annotations.

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

[the equivalence relation]: https://typing.python.org/en/latest/spec/glossary.html#term-equivalent
