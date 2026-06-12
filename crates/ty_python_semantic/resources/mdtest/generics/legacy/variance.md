# Variance: Legacy syntax

Type variables have a property called _variance_ that affects the subtyping and assignability
relations. Much more detail can be found in the [spec]. To summarize, each typevar is either
**covariant**, **contravariant**, **invariant**, or **bivariant**. (Note that bivariance is not
currently mentioned in the typing spec, but is a fourth case that we must consider.)

For all of the examples below, we will consider typevars `T` and `U`, two generic classes using
those typevars `C[T]` and `D[U]`, and two types `A` and `B`.

(Note that dynamic types like `Any` never participate in subtyping, so `C[Any]` is neither a subtype
nor supertype of any other specialization of `C`, regardless of `T`'s variance. It is, however,
assignable to any specialization of `C`, regardless of variance, via materialization.)

## Covariance

With a covariant typevar, subtyping and assignability are in "alignment": if `A <: B` and `C <: D`,
then `C[A] <: C[B]` and `C[A] <: D[B]`.

Types that "produce" data on demand are covariant in their typevar. If you expect a sequence of
`int`s, someone can safely provide a sequence of `bool`s, since each `bool` element that you would
get from the sequence is a valid `int`.

```py
from ty_extensions import is_assignable_to, is_equivalent_to, is_subtype_of, static_assert, Unknown
from typing import Any, Generic, TypeVar

class A: ...
class B(A): ...

T = TypeVar("T", covariant=True)
U = TypeVar("U", covariant=True)

class C(Generic[T]):
    def receive(self) -> T:
        raise ValueError

class D(C[U]):
    pass

static_assert(is_assignable_to(C[B], C[A]))
static_assert(not is_assignable_to(C[A], C[B]))
static_assert(is_assignable_to(C[A], C[Any]))
static_assert(is_assignable_to(C[B], C[Any]))
static_assert(is_assignable_to(C[Any], C[A]))
static_assert(is_assignable_to(C[Any], C[B]))

static_assert(is_assignable_to(D[B], C[A]))
static_assert(not is_assignable_to(D[A], C[B]))
static_assert(is_assignable_to(D[A], C[Any]))
static_assert(is_assignable_to(D[B], C[Any]))
static_assert(is_assignable_to(D[Any], C[A]))
static_assert(is_assignable_to(D[Any], C[B]))

static_assert(is_subtype_of(C[B], C[A]))
static_assert(is_subtype_of(C[A], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
static_assert(not is_subtype_of(C[A], C[Any]))
static_assert(not is_subtype_of(C[B], C[Any]))
static_assert(not is_subtype_of(C[Any], C[A]))
static_assert(not is_subtype_of(C[Any], C[B]))
static_assert(not is_subtype_of(C[Any], C[Any]))

static_assert(is_subtype_of(D[B], C[A]))
static_assert(not is_subtype_of(D[A], C[B]))
static_assert(not is_subtype_of(D[A], C[Any]))
static_assert(not is_subtype_of(D[B], C[Any]))
static_assert(not is_subtype_of(D[Any], C[A]))
static_assert(not is_subtype_of(D[Any], C[B]))

static_assert(is_equivalent_to(C[A], C[A]))
static_assert(is_equivalent_to(C[B], C[B]))
static_assert(not is_equivalent_to(C[B], C[A]))
static_assert(not is_equivalent_to(C[A], C[B]))
static_assert(not is_equivalent_to(C[A], C[Any]))
static_assert(not is_equivalent_to(C[B], C[Any]))
static_assert(not is_equivalent_to(C[Any], C[A]))
static_assert(not is_equivalent_to(C[Any], C[B]))

static_assert(not is_equivalent_to(D[A], C[A]))
static_assert(not is_equivalent_to(D[B], C[B]))
static_assert(not is_equivalent_to(D[B], C[A]))
static_assert(not is_equivalent_to(D[A], C[B]))
static_assert(not is_equivalent_to(D[A], C[Any]))
static_assert(not is_equivalent_to(D[B], C[Any]))
static_assert(not is_equivalent_to(D[Any], C[A]))
static_assert(not is_equivalent_to(D[Any], C[B]))

static_assert(is_equivalent_to(C[Any], C[Any]))
static_assert(is_equivalent_to(C[Any], C[Unknown]))

static_assert(not is_equivalent_to(D[Any], C[Any]))
static_assert(not is_equivalent_to(D[Any], C[Unknown]))
```

## Contravariance

With a contravariant typevar, subtyping and assignability are in "opposition": if `A <: B` and
`C <: D`, then `C[B] <: C[A]` and `D[B] <: C[A]`.

Types that "consume" data are contravariant in their typevar. If you expect a consumer that receives
`bool`s, someone can safely provide a consumer that expects to receive `int`s, since each `bool`
that you pass into the consumer is a valid `int`.

```py
from ty_extensions import is_assignable_to, is_equivalent_to, is_subtype_of, static_assert, Unknown
from typing import Any, Generic, TypeVar

class A: ...
class B(A): ...

T = TypeVar("T", contravariant=True)
U = TypeVar("U", contravariant=True)

class C(Generic[T]):
    def send(self, value: T): ...

class D(C[U]):
    pass

static_assert(not is_assignable_to(C[B], C[A]))
static_assert(is_assignable_to(C[A], C[B]))
static_assert(is_assignable_to(C[A], C[Any]))
static_assert(is_assignable_to(C[B], C[Any]))
static_assert(is_assignable_to(C[Any], C[A]))
static_assert(is_assignable_to(C[Any], C[B]))

static_assert(not is_assignable_to(D[B], C[A]))
static_assert(is_assignable_to(D[A], C[B]))
static_assert(is_assignable_to(D[A], C[Any]))
static_assert(is_assignable_to(D[B], C[Any]))
static_assert(is_assignable_to(D[Any], C[A]))
static_assert(is_assignable_to(D[Any], C[B]))

static_assert(not is_subtype_of(C[B], C[A]))
static_assert(is_subtype_of(C[A], C[B]))
static_assert(not is_subtype_of(C[A], C[Any]))
static_assert(not is_subtype_of(C[B], C[Any]))
static_assert(not is_subtype_of(C[Any], C[A]))
static_assert(not is_subtype_of(C[Any], C[B]))

static_assert(not is_subtype_of(D[B], C[A]))
static_assert(is_subtype_of(D[A], C[B]))
static_assert(not is_subtype_of(D[A], C[Any]))
static_assert(not is_subtype_of(D[B], C[Any]))
static_assert(not is_subtype_of(D[Any], C[A]))
static_assert(not is_subtype_of(D[Any], C[B]))

static_assert(is_equivalent_to(C[A], C[A]))
static_assert(is_equivalent_to(C[B], C[B]))
static_assert(not is_equivalent_to(C[B], C[A]))
static_assert(not is_equivalent_to(C[A], C[B]))
static_assert(not is_equivalent_to(C[A], C[Any]))
static_assert(not is_equivalent_to(C[B], C[Any]))
static_assert(not is_equivalent_to(C[Any], C[A]))
static_assert(not is_equivalent_to(C[Any], C[B]))

static_assert(not is_equivalent_to(D[A], C[A]))
static_assert(not is_equivalent_to(D[B], C[B]))
static_assert(not is_equivalent_to(D[B], C[A]))
static_assert(not is_equivalent_to(D[A], C[B]))
static_assert(not is_equivalent_to(D[A], C[Any]))
static_assert(not is_equivalent_to(D[B], C[Any]))
static_assert(not is_equivalent_to(D[Any], C[A]))
static_assert(not is_equivalent_to(D[Any], C[B]))

static_assert(is_equivalent_to(C[Any], C[Any]))
static_assert(is_equivalent_to(C[Any], C[Unknown]))

static_assert(not is_equivalent_to(D[Any], C[Any]))
static_assert(not is_equivalent_to(D[Any], C[Unknown]))
```

## Bounded typevars in contravariant positions

When a bounded typevar appears in a contravariant position, the actual type doesn't need to satisfy
the bound directly. The typevar can be solved to the intersection of the actual type and the bound
(e.g., `Never` when disjoint).

```py
from typing import Generic, TypeVar

T = TypeVar("T", contravariant=True)
T_int = TypeVar("T_int", bound=int)

class Contra(Generic[T]): ...

def f(x: Contra[T_int]) -> T_int:
    raise NotImplementedError

def _(x: Contra[str]):
    reveal_type(f(x))  # revealed: Never
```

## Invariance

With an invariant typevar, only equivalent specializations of the generic class are subtypes of or
assignable to each other.

This often occurs for types that are both producers _and_ consumers, like a mutable `list`.
Iterating over the elements in a list would work with a covariant typevar, just like with the
"producer" type above. Appending elements to a list would work with a contravariant typevar, just
like with the "consumer" type above. However, a typevar cannot be both covariant and contravariant
at the same time!

If you expect a mutable list of `int`s, it's not safe for someone to provide you with a mutable list
of `bool`s, since you might try to add an element to the list: if you try to add an `int`, the list
would no longer only contain elements that are subtypes of `bool`.

Conversely, if you expect a mutable list of `bool`s, it's not safe for someone to provide you with a
mutable list of `int`s, since you might try to extract elements from the list: you expect every
element that you extract to be a subtype of `bool`, but the list can contain any `int`.

In the end, if you expect a mutable list, you must always be given a list of exactly that type,
since we can't know in advance which of the allowed methods you'll want to use.

```py
from ty_extensions import is_assignable_to, is_equivalent_to, is_subtype_of, static_assert, Unknown
from typing import Any, Generic, TypeVar

class A: ...
class B(A): ...

T = TypeVar("T")
U = TypeVar("U")

class C(Generic[T]):
    def send(self, value: T): ...
    def receive(self) -> T:
        raise ValueError

class D(C[U]):
    pass

static_assert(not is_assignable_to(C[B], C[A]))
static_assert(not is_assignable_to(C[A], C[B]))
static_assert(is_assignable_to(C[A], C[Any]))
static_assert(is_assignable_to(C[B], C[Any]))
static_assert(is_assignable_to(C[Any], C[A]))
static_assert(is_assignable_to(C[Any], C[B]))

static_assert(not is_assignable_to(D[B], C[A]))
static_assert(not is_assignable_to(D[A], C[B]))
static_assert(is_assignable_to(D[A], C[Any]))
static_assert(is_assignable_to(D[B], C[Any]))
static_assert(is_assignable_to(D[Any], C[A]))
static_assert(is_assignable_to(D[Any], C[B]))

static_assert(not is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
static_assert(not is_subtype_of(C[A], C[Any]))
static_assert(not is_subtype_of(C[B], C[Any]))
static_assert(not is_subtype_of(C[Any], C[A]))
static_assert(not is_subtype_of(C[Any], C[B]))

static_assert(not is_subtype_of(D[B], C[A]))
static_assert(not is_subtype_of(D[A], C[B]))
static_assert(not is_subtype_of(D[A], C[Any]))
static_assert(not is_subtype_of(D[B], C[Any]))
static_assert(not is_subtype_of(D[Any], C[A]))
static_assert(not is_subtype_of(D[Any], C[B]))

static_assert(is_equivalent_to(C[A], C[A]))
static_assert(is_equivalent_to(C[B], C[B]))
static_assert(not is_equivalent_to(C[B], C[A]))
static_assert(not is_equivalent_to(C[A], C[B]))
static_assert(not is_equivalent_to(C[A], C[Any]))
static_assert(not is_equivalent_to(C[B], C[Any]))
static_assert(not is_equivalent_to(C[Any], C[A]))
static_assert(not is_equivalent_to(C[Any], C[B]))

static_assert(not is_equivalent_to(D[A], C[A]))
static_assert(not is_equivalent_to(D[B], C[B]))
static_assert(not is_equivalent_to(D[B], C[A]))
static_assert(not is_equivalent_to(D[A], C[B]))
static_assert(not is_equivalent_to(D[A], C[Any]))
static_assert(not is_equivalent_to(D[B], C[Any]))
static_assert(not is_equivalent_to(D[Any], C[A]))
static_assert(not is_equivalent_to(D[Any], C[B]))

static_assert(is_equivalent_to(C[Any], C[Any]))
static_assert(is_equivalent_to(C[Any], C[Unknown]))

static_assert(not is_equivalent_to(D[Any], C[Any]))
static_assert(not is_equivalent_to(D[Any], C[Unknown]))
```

## Bivariance

With a bivariant typevar, _all_ specializations of the generic class are assignable to (and in fact,
gradually equivalent to) each other, and all fully static specializations are subtypes of (and
equivalent to) each other.

It is not possible to construct a legacy typevar that is explicitly bivariant.

## Inheriting from generic classes with explicit variance

A generic subclass cannot claim a variance that is less restrictive than the variance required by
one of its specialized bases. This validation also applies after composing nested generic types or
resolving aliases used as bases.

```py
from typing import Generic, TypeAlias, TypeVar

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)
T_contra = TypeVar("T_contra", contravariant=True)

class Invariant(Generic[T]): ...
class Covariant(Generic[T_co]): ...
class Contravariant(Generic[T_contra]): ...
class CoContra(Generic[T_co, T_contra]): ...
class GoodInvariantInCovariant(Covariant[T]): ...
class GoodInvariantInContravariant(Contravariant[T]): ...
class GoodCovariant(Covariant[T_co]): ...
class GoodContravariant(Contravariant[T_contra]): ...
class GoodNested(Contravariant[Contravariant[T_co]]): ...

# snapshot: invalid-generic-class
class BadInvariantCo(Invariant[T_co]): ...

# snapshot: invalid-generic-class
class BadInvariantContra(Invariant[T_contra]): ...

# snapshot: invalid-generic-class
class BadCovariant(Covariant[T_contra]): ...

# snapshot: invalid-generic-class
class BadContravariant(Contravariant[T_co]): ...

# error: [invalid-generic-class]
class BadNested(Contravariant[Covariant[T_co]]): ...

# error: [invalid-generic-class]
class BadComposed(Contravariant[Covariant[Contravariant[T_contra]]]): ...

# error: [invalid-generic-class]
class BadSecond(CoContra[T_co, T_co]): ...

# error: [invalid-generic-class]
class BadFirst(CoContra[T_contra, T_contra]): ...

# error: [invalid-generic-class]
class BadSecondNested(CoContra[Covariant[T_co], Covariant[T_co]]): ...

InvariantAlias: TypeAlias = Invariant[T_co]
CovariantAlias: TypeAlias = Covariant[T_co]
NestedAlias: TypeAlias = Contravariant[T_contra]

class GoodAlias(CovariantAlias[T_co]): ...

# error: [invalid-generic-class]
class BadAlias(InvariantAlias[T_co]): ...

# error: [invalid-generic-class]
class BadNestedAlias(NestedAlias[NestedAlias[NestedAlias[T_co]]]): ...
```

```snapshot
error[invalid-generic-class]: Variance of type variable `T_co` is incompatible with base class `Invariant`
  --> src/mdtest_snippet.py:18:22
   |
18 | class BadInvariantCo(Invariant[T_co]): ...
   |                      ^^^^^^^^^^^^^^^
   |
help: Type variable `T_co` is declared as covariant, but base class `Invariant` requires it to be invariant


error[invalid-generic-class]: Variance of type variable `T_contra` is incompatible with base class `Invariant`
  --> src/mdtest_snippet.py:21:26
   |
21 | class BadInvariantContra(Invariant[T_contra]): ...
   |                          ^^^^^^^^^^^^^^^^^^^
   |
help: Type variable `T_contra` is declared as contravariant, but base class `Invariant` requires it to be invariant


error[invalid-generic-class]: Variance of type variable `T_contra` is incompatible with base class `Covariant`
  --> src/mdtest_snippet.py:24:20
   |
24 | class BadCovariant(Covariant[T_contra]): ...
   |                    ^^^^^^^^^^^^^^^^^^^
   |
help: Type variable `T_contra` is declared as contravariant, but base class `Covariant` requires it to be covariant


error[invalid-generic-class]: Variance of type variable `T_co` is incompatible with base class `Contravariant`
  --> src/mdtest_snippet.py:27:24
   |
27 | class BadContravariant(Contravariant[T_co]): ...
   |                        ^^^^^^^^^^^^^^^^^^^
   |
help: Type variable `T_co` is declared as covariant, but base class `Contravariant` requires it to be contravariant
```

## Inferred variance

Legacy type variables with inferred variance are validated according to their uses, rather than as
if they had an explicit invariant declaration.

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import is_assignable_to, static_assert
from typing import Generic, TypeVar

class A: ...
class B(A): ...

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)
T_infer = TypeVar("T_infer", infer_variance=True)

class Invariant(Generic[T]): ...
class Covariant(Generic[T_co]): ...
class GoodInferredInvariant(Invariant[T_infer]): ...
class GoodInferredCovariant(Covariant[T_infer]): ...

static_assert(not is_assignable_to(GoodInferredInvariant[B], GoodInferredInvariant[A]))
static_assert(not is_assignable_to(GoodInferredInvariant[A], GoodInferredInvariant[B]))
```

[spec]: https://typing.python.org/en/latest/spec/generics.html#variance
