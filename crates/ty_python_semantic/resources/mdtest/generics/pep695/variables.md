# PEP 695 Generics

```toml
[environment]
python-version = "3.12"
```

[PEP 695] and Python 3.12 introduced new, more ergonomic syntax for type variables.

## Type variables

### Defining PEP 695 type variables

PEP 695 introduces a new syntax for defining type variables. The resulting type variables are
instances of `typing.TypeVar`, just like legacy type variables.

```py
def f[T]():
    reveal_type(type(T))  # revealed: <class 'TypeVar'>
    reveal_type(T)  # revealed: typing.TypeVar
    reveal_type(T.__name__)  # revealed: Literal["T"]
```

### Type variables with a default

Note that the `__default__` property is only available in Python ≥3.13.

```toml
[environment]
python-version = "3.13"
```

```py
def f[T = int]():
    reveal_type(T.__default__)  # revealed: int
    reveal_type(T.__bound__)  # revealed: None
    reveal_type(T.__constraints__)  # revealed: tuple[()]

def g[S]():
    reveal_type(S.__default__)  # revealed: NoDefault
```

### Type variables with an upper bound

```py
def f[T: int]():
    reveal_type(T.__bound__)  # revealed: int
    reveal_type(T.__constraints__)  # revealed: tuple[()]

def g[S]():
    reveal_type(S.__bound__)  # revealed: None
```

### Type variables with constraints

```py
def f[T: (int, str)]():
    reveal_type(T.__constraints__)  # revealed: tuple[int, str]
    reveal_type(T.__bound__)  # revealed: None

def g[S]():
    reveal_type(S.__constraints__)  # revealed: tuple[()]
```

### Cannot have only one constraint

> `TypeVar` supports constraining parametric types to a fixed set of possible types...There should
> be at least two constraints, if any; specifying a single constraint is disallowed.

```py
# error: [invalid-type-variable-constraints] "TypeVar must have at least two constrained types"
def f[T: (int,)]():
    pass
```

## Invalid uses

Note that many of the invalid uses of legacy typevars do not apply to PEP 695 typevars, since the
PEP 695 syntax is only allowed places where typevars are allowed.

## Displaying typevars

We use a suffix when displaying the typevars of a generic function or class. This helps distinguish
different uses of the same typevar.

```py
def f[T](x: T, y: T) -> None:
    # TODO: revealed: T@f
    reveal_type(x)  # revealed: T

class C[T]:
    def m(self, x: T) -> None:
        # TODO: revealed: T@c
        reveal_type(x)  # revealed: T
```

## Fully static typevars

We consider a typevar to be fully static unless it has a non-fully-static bound or constraint. This
is true even though a fully static typevar might be specialized to a gradual form like `Any`. (This
is similar to how you can assign an expression whose type is not fully static to a target whose type
is.)

```py
from ty_extensions import is_fully_static, static_assert
from typing import Any

def unbounded_unconstrained[T](t: T) -> None:
    static_assert(is_fully_static(T))

def bounded[T: int](t: T) -> None:
    static_assert(is_fully_static(T))

def bounded_by_gradual[T: Any](t: T) -> None:
    static_assert(not is_fully_static(T))

def constrained[T: (int, str)](t: T) -> None:
    static_assert(is_fully_static(T))

def constrained_by_gradual[T: (int, Any)](t: T) -> None:
    static_assert(not is_fully_static(T))
```

## Subtyping and assignability

(Note: for simplicity, all of the prose in this section refers to _subtyping_ involving fully static
typevars. Unless otherwise noted, all of the claims also apply to _assignability_ involving gradual
typevars.)

We can make no assumption about what type an unbounded, unconstrained, fully static typevar will be
specialized to. Properties are true of the typevar only if they are true for every valid
specialization. Thus, the typevar is a subtype of itself and of `object`, but not of any other type
(including other typevars).

```py
from ty_extensions import is_assignable_to, is_subtype_of, static_assert

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class Unrelated: ...

def unbounded_unconstrained[T, U](t: T, u: U) -> None:
    static_assert(is_assignable_to(T, T))
    static_assert(is_assignable_to(T, object))
    static_assert(not is_assignable_to(T, Super))
    static_assert(is_assignable_to(U, U))
    static_assert(is_assignable_to(U, object))
    static_assert(not is_assignable_to(U, Super))
    static_assert(not is_assignable_to(T, U))
    static_assert(not is_assignable_to(U, T))

    static_assert(is_subtype_of(T, T))
    static_assert(is_subtype_of(T, object))
    static_assert(not is_subtype_of(T, Super))
    static_assert(is_subtype_of(U, U))
    static_assert(is_subtype_of(U, object))
    static_assert(not is_subtype_of(U, Super))
    static_assert(not is_subtype_of(T, U))
    static_assert(not is_subtype_of(U, T))
```

A bounded typevar is assignable to its bound, and a bounded, fully static typevar is a subtype of
its bound. (A typevar with a non-fully-static bound is itself non-fully-static, and therefore does
not participate in subtyping.) A fully static bound is not assignable to, nor a subtype of, the
typevar, since the typevar might be specialized to a smaller type. (This is true even if the bound
is a final class, since the typevar can still be specialized to `Never`.)

```py
from typing import Any
from typing_extensions import final

def bounded[T: Super](t: T) -> None:
    static_assert(is_assignable_to(T, Super))
    static_assert(not is_assignable_to(T, Sub))
    static_assert(not is_assignable_to(Super, T))
    static_assert(not is_assignable_to(Sub, T))

    static_assert(is_subtype_of(T, Super))
    static_assert(not is_subtype_of(T, Sub))
    static_assert(not is_subtype_of(Super, T))
    static_assert(not is_subtype_of(Sub, T))

def bounded_by_gradual[T: Any](t: T) -> None:
    static_assert(is_assignable_to(T, Any))
    static_assert(is_assignable_to(Any, T))
    static_assert(is_assignable_to(T, Super))
    static_assert(not is_assignable_to(Super, T))
    static_assert(is_assignable_to(T, Sub))
    static_assert(not is_assignable_to(Sub, T))

    static_assert(not is_subtype_of(T, Any))
    static_assert(not is_subtype_of(Any, T))
    static_assert(not is_subtype_of(T, Super))
    static_assert(not is_subtype_of(Super, T))
    static_assert(not is_subtype_of(T, Sub))
    static_assert(not is_subtype_of(Sub, T))

@final
class FinalClass: ...

def bounded_final[T: FinalClass](t: T) -> None:
    static_assert(is_assignable_to(T, FinalClass))
    static_assert(not is_assignable_to(FinalClass, T))

    static_assert(is_subtype_of(T, FinalClass))
    static_assert(not is_subtype_of(FinalClass, T))
```

Two distinct fully static typevars are not subtypes of each other, even if they have the same
bounds, since there is (still) no guarantee that they will be specialized to the same type. This is
true even if both typevars are bounded by the same final class, since you can specialize the
typevars to `Never` in addition to that final class.

```py
def two_bounded[T: Super, U: Super](t: T, u: U) -> None:
    static_assert(not is_assignable_to(T, U))
    static_assert(not is_assignable_to(U, T))

    static_assert(not is_subtype_of(T, U))
    static_assert(not is_subtype_of(U, T))

def two_final_bounded[T: FinalClass, U: FinalClass](t: T, u: U) -> None:
    static_assert(not is_assignable_to(T, U))
    static_assert(not is_assignable_to(U, T))

    static_assert(not is_subtype_of(T, U))
    static_assert(not is_subtype_of(U, T))
```

A constrained fully static typevar is assignable to the union of its constraints, but not to any of
the constraints individually. None of the constraints are subtypes of the typevar, though the
intersection of all of its constraints is a subtype of the typevar.

```py
from ty_extensions import Intersection

def constrained[T: (Base, Unrelated)](t: T) -> None:
    static_assert(not is_assignable_to(T, Super))
    static_assert(not is_assignable_to(T, Base))
    static_assert(not is_assignable_to(T, Sub))
    static_assert(not is_assignable_to(T, Unrelated))
    static_assert(is_assignable_to(T, Super | Unrelated))
    static_assert(is_assignable_to(T, Base | Unrelated))
    static_assert(not is_assignable_to(T, Sub | Unrelated))
    static_assert(not is_assignable_to(Super, T))
    static_assert(not is_assignable_to(Unrelated, T))
    static_assert(not is_assignable_to(Super | Unrelated, T))
    static_assert(is_assignable_to(Intersection[Base, Unrelated], T))

    static_assert(not is_subtype_of(T, Super))
    static_assert(not is_subtype_of(T, Base))
    static_assert(not is_subtype_of(T, Sub))
    static_assert(not is_subtype_of(T, Unrelated))
    static_assert(is_subtype_of(T, Super | Unrelated))
    static_assert(is_subtype_of(T, Base | Unrelated))
    static_assert(not is_subtype_of(T, Sub | Unrelated))
    static_assert(not is_subtype_of(Super, T))
    static_assert(not is_subtype_of(Unrelated, T))
    static_assert(not is_subtype_of(Super | Unrelated, T))
    static_assert(is_subtype_of(Intersection[Base, Unrelated], T))

def constrained_by_gradual[T: (Base, Any)](t: T) -> None:
    static_assert(is_assignable_to(T, Super))
    static_assert(is_assignable_to(T, Base))
    static_assert(not is_assignable_to(T, Sub))
    static_assert(not is_assignable_to(T, Unrelated))
    static_assert(is_assignable_to(T, Any))
    static_assert(is_assignable_to(T, Super | Any))
    static_assert(is_assignable_to(T, Super | Unrelated))
    static_assert(not is_assignable_to(Super, T))
    static_assert(is_assignable_to(Base, T))
    static_assert(not is_assignable_to(Unrelated, T))
    static_assert(is_assignable_to(Any, T))
    static_assert(not is_assignable_to(Super | Any, T))
    static_assert(is_assignable_to(Base | Any, T))
    static_assert(not is_assignable_to(Super | Unrelated, T))
    static_assert(is_assignable_to(Intersection[Base, Unrelated], T))
    static_assert(is_assignable_to(Intersection[Base, Any], T))

    static_assert(not is_subtype_of(T, Super))
    static_assert(not is_subtype_of(T, Base))
    static_assert(not is_subtype_of(T, Sub))
    static_assert(not is_subtype_of(T, Unrelated))
    static_assert(not is_subtype_of(T, Any))
    static_assert(not is_subtype_of(T, Super | Any))
    static_assert(not is_subtype_of(T, Super | Unrelated))
    static_assert(not is_subtype_of(Super, T))
    static_assert(not is_subtype_of(Base, T))
    static_assert(not is_subtype_of(Unrelated, T))
    static_assert(not is_subtype_of(Any, T))
    static_assert(not is_subtype_of(Super | Any, T))
    static_assert(not is_subtype_of(Base | Any, T))
    static_assert(not is_subtype_of(Super | Unrelated, T))
    static_assert(not is_subtype_of(Intersection[Base, Unrelated], T))
    static_assert(not is_subtype_of(Intersection[Base, Any], T))
```

Two distinct fully static typevars are not subtypes of each other, even if they have the same
constraints, and even if any of the constraints are final. There must always be at least two
distinct constraints, meaning that there is (still) no guarantee that they will be specialized to
the same type.

```py
def two_constrained[T: (int, str), U: (int, str)](t: T, u: U) -> None:
    static_assert(not is_assignable_to(T, U))
    static_assert(not is_assignable_to(U, T))

    static_assert(not is_subtype_of(T, U))
    static_assert(not is_subtype_of(U, T))

@final
class AnotherFinalClass: ...

def two_final_constrained[T: (FinalClass, AnotherFinalClass), U: (FinalClass, AnotherFinalClass)](t: T, u: U) -> None:
    static_assert(not is_assignable_to(T, U))
    static_assert(not is_assignable_to(U, T))

    static_assert(not is_subtype_of(T, U))
    static_assert(not is_subtype_of(U, T))
```

A bound or constrained typevar is a subtype of itself in a union:

```py
def union[T: Base, U: (Base, Unrelated)](t: T, u: U) -> None:
    static_assert(is_assignable_to(T, T | None))
    static_assert(is_assignable_to(U, U | None))

    static_assert(is_subtype_of(T, T | None))
    static_assert(is_subtype_of(U, U | None))
```

And an intersection of a typevar with another type is always a subtype of the TypeVar:

```py
from ty_extensions import Intersection, Not, is_disjoint_from

class A: ...

def inter[T: Base, U: (Base, Unrelated)](t: T, u: U) -> None:
    static_assert(is_assignable_to(Intersection[T, Unrelated], T))
    static_assert(is_subtype_of(Intersection[T, Unrelated], T))

    static_assert(is_assignable_to(Intersection[U, A], U))
    static_assert(is_subtype_of(Intersection[U, A], U))

    static_assert(is_disjoint_from(Not[T], T))
    static_assert(is_disjoint_from(T, Not[T]))
    static_assert(is_disjoint_from(Not[U], U))
    static_assert(is_disjoint_from(U, Not[U]))
```

## Equivalence

A fully static `TypeVar` is always equivalent to itself, but never to another `TypeVar`, since there
is no guarantee that they will be specialized to the same type. (This is true even if both typevars
are bounded by the same final class, since you can specialize the typevars to `Never` in addition to
that final class.)

```py
from typing import final
from ty_extensions import is_equivalent_to, static_assert, is_gradual_equivalent_to

@final
class FinalClass: ...

@final
class SecondFinalClass: ...

def f[A, B, C: FinalClass, D: FinalClass, E: (FinalClass, SecondFinalClass), F: (FinalClass, SecondFinalClass)]():
    static_assert(is_equivalent_to(A, A))
    static_assert(is_equivalent_to(B, B))
    static_assert(is_equivalent_to(C, C))
    static_assert(is_equivalent_to(D, D))
    static_assert(is_equivalent_to(E, E))
    static_assert(is_equivalent_to(F, F))

    static_assert(is_gradual_equivalent_to(A, A))
    static_assert(is_gradual_equivalent_to(B, B))
    static_assert(is_gradual_equivalent_to(C, C))
    static_assert(is_gradual_equivalent_to(D, D))
    static_assert(is_gradual_equivalent_to(E, E))
    static_assert(is_gradual_equivalent_to(F, F))

    static_assert(not is_equivalent_to(A, B))
    static_assert(not is_equivalent_to(C, D))
    static_assert(not is_equivalent_to(E, F))

    static_assert(not is_gradual_equivalent_to(A, B))
    static_assert(not is_gradual_equivalent_to(C, D))
    static_assert(not is_gradual_equivalent_to(E, F))
```

TypeVars which have non-fully-static bounds or constraints do not participate in equivalence
relations, but do participate in gradual equivalence relations.

```py
from typing import final, Any
from ty_extensions import is_equivalent_to, static_assert, is_gradual_equivalent_to

# fmt: off

def f[
    A: tuple[Any],
    B: tuple[Any],
    C: (tuple[Any], tuple[Any, Any]),
    D: (tuple[Any], tuple[Any, Any])
]():
    static_assert(not is_equivalent_to(A, A))
    static_assert(not is_equivalent_to(B, B))
    static_assert(not is_equivalent_to(C, C))
    static_assert(not is_equivalent_to(D, D))

    static_assert(is_gradual_equivalent_to(A, A))
    static_assert(is_gradual_equivalent_to(B, B))
    static_assert(is_gradual_equivalent_to(C, C))
    static_assert(is_gradual_equivalent_to(D, D))

# fmt: on
```

## Singletons and single-valued types

(Note: for simplicity, all of the prose in this section refers to _singleton_ types, but all of the
claims also apply to _single-valued_ types.)

An unbounded, unconstrained typevar is not a singleton, because it can be specialized to a
non-singleton type.

```py
from ty_extensions import is_singleton, is_single_valued, static_assert

def unbounded_unconstrained[T](t: T) -> None:
    static_assert(not is_singleton(T))
    static_assert(not is_single_valued(T))
```

A bounded typevar is not a singleton, even if its bound is a singleton, since it can still be
specialized to `Never`.

```py
def bounded[T: None](t: T) -> None:
    static_assert(not is_singleton(T))
    static_assert(not is_single_valued(T))
```

A constrained typevar is a singleton if all of its constraints are singletons. (Note that you cannot
specialize a constrained typevar to a subtype of a constraint.)

```py
from typing_extensions import Literal

def constrained_non_singletons[T: (int, str)](t: T) -> None:
    static_assert(not is_singleton(T))
    static_assert(not is_single_valued(T))

def constrained_singletons[T: (Literal[True], Literal[False])](t: T) -> None:
    static_assert(is_singleton(T))

def constrained_single_valued[T: (Literal[True], tuple[()])](t: T) -> None:
    static_assert(is_single_valued(T))
```

## Unions involving typevars

The union of an unbounded unconstrained typevar with any other type cannot be simplified, since
there is no guarantee what type the typevar will be specialized to.

```py
from typing import Any

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class Unrelated: ...

def unbounded_unconstrained[T](t: T) -> None:
    def _(x: T | Super) -> None:
        reveal_type(x)  # revealed: T | Super

    def _(x: T | Base) -> None:
        reveal_type(x)  # revealed: T | Base

    def _(x: T | Sub) -> None:
        reveal_type(x)  # revealed: T | Sub

    def _(x: T | Unrelated) -> None:
        reveal_type(x)  # revealed: T | Unrelated

    def _(x: T | Any) -> None:
        reveal_type(x)  # revealed: T | Any
```

The union of a bounded typevar with its bound is that bound. (The typevar is guaranteed to be
specialized to a subtype of the bound.) The union of a bounded typevar with a subtype of its bound
cannot be simplified. (The typevar might be specialized to a different subtype of the bound.)

```py
def bounded[T: Base](t: T) -> None:
    def _(x: T | Super) -> None:
        reveal_type(x)  # revealed: Super

    def _(x: T | Base) -> None:
        reveal_type(x)  # revealed: Base

    def _(x: T | Sub) -> None:
        reveal_type(x)  # revealed: T | Sub

    def _(x: T | Unrelated) -> None:
        reveal_type(x)  # revealed: T | Unrelated

    def _(x: T | Any) -> None:
        reveal_type(x)  # revealed: T | Any
```

The union of a constrained typevar with a type depends on how that type relates to the constraints.
If all of the constraints are a subtype of that type, the union simplifies to that type. Inversely,
if the type is a subtype of every constraint, the union simplifies to the typevar. Otherwise, the
union cannot be simplified.

```py
def constrained[T: (Base, Sub)](t: T) -> None:
    def _(x: T | Super) -> None:
        reveal_type(x)  # revealed: Super

    def _(x: T | Base) -> None:
        reveal_type(x)  # revealed: Base

    def _(x: T | Sub) -> None:
        reveal_type(x)  # revealed: T

    def _(x: T | Unrelated) -> None:
        reveal_type(x)  # revealed: T | Unrelated

    def _(x: T | Any) -> None:
        reveal_type(x)  # revealed: T | Any
```

## Intersections involving typevars

The intersection of an unbounded unconstrained typevar with any other type cannot be simplified,
since there is no guarantee what type the typevar will be specialized to.

```py
from ty_extensions import Intersection
from typing import Any

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class Unrelated: ...

def unbounded_unconstrained[T](t: T) -> None:
    def _(x: Intersection[T, Super]) -> None:
        reveal_type(x)  # revealed: T & Super

    def _(x: Intersection[T, Base]) -> None:
        reveal_type(x)  # revealed: T & Base

    def _(x: Intersection[T, Sub]) -> None:
        reveal_type(x)  # revealed: T & Sub

    def _(x: Intersection[T, Unrelated]) -> None:
        reveal_type(x)  # revealed: T & Unrelated

    def _(x: Intersection[T, Any]) -> None:
        reveal_type(x)  # revealed: T & Any
```

The intersection of a bounded typevar with its bound or a supertype of its bound is the typevar
itself. (The typevar might be specialized to a subtype of the bound.) The intersection of a bounded
typevar with a subtype of its bound cannot be simplified. (The typevar might be specialized to a
different subtype of the bound.) The intersection of a bounded typevar with a type that is disjoint
from its bound is `Never`.

```py
def bounded[T: Base](t: T) -> None:
    def _(x: Intersection[T, Super]) -> None:
        reveal_type(x)  # revealed: T

    def _(x: Intersection[T, Base]) -> None:
        reveal_type(x)  # revealed: T

    def _(x: Intersection[T, Sub]) -> None:
        reveal_type(x)  # revealed: T & Sub

    def _(x: Intersection[T, None]) -> None:
        reveal_type(x)  # revealed: Never

    def _(x: Intersection[T, Any]) -> None:
        reveal_type(x)  # revealed: T & Any
```

Constrained typevars can be modeled using a hypothetical `OneOf` connector, where the typevar must
be specialized to _one_ of its constraints. The typevar is not the _union_ of those constraints,
since that would allow the typevar to take on values from _multiple_ constraints simultaneously. The
`OneOf` connector would not be a “type” according to a strict reading of the typing spec, since it
would not represent a single set of runtime objects; it would instead represent a _set of_ sets of
runtime objects. This is one reason we have not actually added this connector to our data model yet.
Nevertheless, describing constrained typevars this way helps explain how we simplify intersections
involving them.

This means that when intersecting a constrained typevar with a type `T`, constraints that are
supertypes of `T` can be simplified to `T`, since intersection distributes over `OneOf`. Moreover,
constraints that are disjoint from `T` are no longer valid specializations of the typevar, since
`Never` is an identity for `OneOf`. After these simplifications, if only one constraint remains, we
can simplify the intersection as a whole to that constraint.

```py
def constrained[T: (Base, Sub, Unrelated)](t: T) -> None:
    def _(x: Intersection[T, Base]) -> None:
        # With OneOf this would be OneOf[Base, Sub]
        reveal_type(x)  # revealed: T & Base

    def _(x: Intersection[T, Unrelated]) -> None:
        reveal_type(x)  # revealed: Unrelated

    def _(x: Intersection[T, Sub]) -> None:
        reveal_type(x)  # revealed: Sub

    def _(x: Intersection[T, None]) -> None:
        reveal_type(x)  # revealed: Never

    def _(x: Intersection[T, Any]) -> None:
        reveal_type(x)  # revealed: T & Any
```

We can simplify the intersection similarly when removing a type from a constrained typevar, since
this is modeled internally as an intersection with a negation.

```py
from ty_extensions import Not

def remove_constraint[T: (int, str, bool)](t: T) -> None:
    def _(x: Intersection[T, Not[int]]) -> None:
        reveal_type(x)  # revealed: str & ~int

    def _(x: Intersection[T, Not[str]]) -> None:
        # With OneOf this would be OneOf[int, bool]
        reveal_type(x)  # revealed: T & ~str

    def _(x: Intersection[T, Not[bool]]) -> None:
        reveal_type(x)  # revealed: T & ~bool

    def _(x: Intersection[T, Not[int], Not[str]]) -> None:
        reveal_type(x)  # revealed: Never

    def _(x: Intersection[T, Not[None]]) -> None:
        reveal_type(x)  # revealed: T

    def _(x: Intersection[T, Not[Any]]) -> None:
        reveal_type(x)  # revealed: T & Any
```

The intersection of a typevar with any other type is assignable to (and if fully static, a subtype
of) itself.

```py
from ty_extensions import is_assignable_to, is_subtype_of, static_assert, Not

def intersection_is_assignable[T](t: T) -> None:
    static_assert(is_assignable_to(Intersection[T, None], T))
    static_assert(is_assignable_to(Intersection[T, Not[None]], T))

    static_assert(is_subtype_of(Intersection[T, None], T))
    static_assert(is_subtype_of(Intersection[T, Not[None]], T))
```

## Narrowing

We can use narrowing expressions to eliminate some of the possibilities of a constrained typevar:

```py
class P: ...
class Q: ...
class R: ...

def f[T: (P, Q)](t: T) -> None:
    if isinstance(t, P):
        reveal_type(t)  # revealed: P
        p: P = t
    else:
        reveal_type(t)  # revealed: Q & ~P
        q: Q = t

    if isinstance(t, Q):
        reveal_type(t)  # revealed: Q
        q: Q = t
    else:
        reveal_type(t)  # revealed: P & ~Q
        p: P = t

def g[T: (P, Q, R)](t: T) -> None:
    if isinstance(t, P):
        reveal_type(t)  # revealed: P
        p: P = t
    elif isinstance(t, Q):
        reveal_type(t)  # revealed: Q & ~P
        q: Q = t
    else:
        reveal_type(t)  # revealed: R & ~P & ~Q
        r: R = t

    if isinstance(t, P):
        reveal_type(t)  # revealed: P
        p: P = t
    elif isinstance(t, Q):
        reveal_type(t)  # revealed: Q & ~P
        q: Q = t
    elif isinstance(t, R):
        reveal_type(t)  # revealed: R & ~P & ~Q
        r: R = t
    else:
        reveal_type(t)  # revealed: Never
```

If the constraints are disjoint, simplification does eliminate the redundant negative:

```py
def h[T: (P, None)](t: T) -> None:
    if t is None:
        reveal_type(t)  # revealed: None
        p: None = t
    else:
        reveal_type(t)  # revealed: P
        p: P = t
```

[pep 695]: https://peps.python.org/pep-0695/
