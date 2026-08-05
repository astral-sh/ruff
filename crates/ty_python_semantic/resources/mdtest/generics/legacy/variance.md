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
from ty_extensions import static_assert
from ty_extensions._internal import Unknown, is_assignable_to, is_equivalent_to, is_subtype_of
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
from ty_extensions import static_assert
from ty_extensions._internal import Unknown, is_assignable_to, is_equivalent_to, is_subtype_of
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
from ty_extensions import static_assert
from ty_extensions._internal import Unknown, is_assignable_to, is_equivalent_to, is_subtype_of
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

## Generic protocols require matching variance

A protocol's declared type-variable variance must agree with the positions where its interface uses
that variable. Unlike nominal generic classes, a protocol cannot declare an input-only or
output-only type variable as invariant.

```py
from typing import Protocol, TypeVar

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)
T_contra = TypeVar("T_contra", contravariant=True)

# error: [invalid-protocol] "Type variable `T` in protocol `InvariantSource` should be covariant, but is invariant"
class InvariantSource(Protocol[T]):
    def read(self) -> T: ...

# error: [invalid-protocol] "Type variable `T` in protocol `InvariantSink` should be contravariant, but is invariant"
class InvariantSink(Protocol[T]):
    def write(self, value: T) -> None: ...

# error: [invalid-protocol] "Type variable `T_co` in protocol `CovariantSink` should be contravariant, but is covariant"
class CovariantSink(Protocol[T_co]):
    def write(self, value: T_co) -> None: ...

# error: [invalid-protocol] "Type variable `T_contra` in protocol `ContravariantSource` should be covariant, but is contravariant"
class ContravariantSource(Protocol[T_contra]):
    def read(self) -> T_contra: ...

class CovariantSource(Protocol[T_co]):
    def read(self) -> T_co: ...

class ContravariantSink(Protocol[T_contra]):
    def write(self, value: T_contra) -> None: ...

class InvariantReadWrite(Protocol[T]):
    def read(self) -> T: ...
    def write(self, value: T) -> None: ...
```

## Generic protocol variance follows properties and mutable attributes

A read-only property uses its type variable covariantly. A writable property or mutable attribute
uses the same variable both covariantly and contravariantly, so it requires invariance.

```py
from typing import Callable, Protocol, TypeVar

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)

class ReadOnlyProperty(Protocol[T_co]):
    @property
    def value(self) -> T_co: ...

class WritableProperty(Protocol[T]):
    @property
    def value(self) -> T: ...
    @value.setter
    def value(self, value: T) -> None: ...

class MutableAttribute(Protocol[T]):
    value: T

class MutableDunderAttribute(Protocol[T]):
    __call__: Callable[..., T]

# error: [invalid-protocol] "Type variable `T_co` in protocol `CovariantDunderAttribute` should be invariant, but is covariant"
class CovariantDunderAttribute(Protocol[T_co]):
    __call__: Callable[..., T_co]

class CovariantDunderMethod(Protocol[T_co]):
    def __call__(self) -> T_co: ...

class WritableClassValue(Protocol[T]):
    @property
    def value(self) -> type[T]: ...
    @value.setter
    def value(self, value: type[T]) -> None: ...

class WritableClassAttribute(Protocol[T]):
    @property
    def __class__(self) -> type[T]: ...
    @__class__.setter
    def __class__(self, value: type[T]) -> None: ...

class InvariantReturn(Protocol[T]):
    def read(self) -> list[T]: ...
```

Explicit receiver annotations on property accessors do not influence the variance of the values that
the property accepts.

```py
T_contra = TypeVar("T_contra", contravariant=True)

class ContravariantProperty(Protocol[T_contra]):
    @property
    def value(self: "ContravariantProperty[T_contra]") -> object: ...
    @value.setter
    def value(self: "ContravariantProperty[T_contra]", value: T_contra) -> None: ...
```

## Generic protocol variance ignores constructors and receiver annotations

Constructors do not belong to a protocol's structural interface. If a type variable appears only in
a constructor, its inferred variance therefore falls back to covariance.

```py
from typing import Protocol, TypeVar

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)

# error: [invalid-protocol] "Type variable `T` in protocol `ConstructorOnly` should be covariant, but is invariant"
class ConstructorOnly(Protocol[T]):
    def __init__(self, value: T) -> None: ...

class CovariantConstructorOnly(Protocol[T_co]):
    def __init__(self, value: T_co) -> None: ...

class ExcludedClassGetitem(Protocol[T_co]):
    @classmethod
    def __class_getitem__(cls, value: T_co) -> object: ...
```

Explicit instance and class receivers identify where a method is bound; they do not make a
contravariant protocol type variable appear covariantly.

```py
T_contra = TypeVar("T_contra", contravariant=True)

class ExplicitReceivers(Protocol[T_contra]):
    def send(self: "ExplicitReceivers[T_contra]", value: T_contra) -> None: ...
    @classmethod
    def configure(cls: "type[ExplicitReceivers[T_contra]]") -> None: ...
```

## Parameter specifications and inferred protocol variance

Protocol variance validation applies only to explicitly declared regular type variables. Parameter
specifications and variables whose variance is inferred do not need matching declarations.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ParamSpec, Protocol, TypeVar

P = ParamSpec("P")
R_co = TypeVar("R_co", covariant=True)
T_infer = TypeVar("T_infer", infer_variance=True)

class Callback(Protocol[P, R_co]):
    def __call__(self, *args: P.args, **kwargs: P.kwargs) -> R_co: ...

class InferredSource(Protocol[T_infer]):
    def read(self) -> T_infer: ...
```

## Invalid protocol headers do not cause cascading variance diagnostics

Malformed generic parameter ordering and duplicate generic bases are diagnosed separately. Ignoring
those diagnostics does not turn them into protocol-variance errors.

```toml
[environment]
python-version = "3.13"

[rules]
invalid-generic-class = "ignore"
```

```py
from typing import Generic, Protocol, TypeVar

DefaultT = TypeVar("DefaultT", default=int)
T = TypeVar("T")

class InvalidParameterOrder(Protocol[DefaultT, T]): ...
class DuplicateGenericBase(Protocol[T], Generic[T]): ...
```

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
help: Type variable `T_co` is declared as covariant, but base class `Invariant` requires it to be invariant


error[invalid-generic-class]: Variance of type variable `T_contra` is incompatible with base class `Invariant`
  --> src/mdtest_snippet.py:21:26
   |
21 | class BadInvariantContra(Invariant[T_contra]): ...
   |                          ^^^^^^^^^^^^^^^^^^^
help: Type variable `T_contra` is declared as contravariant, but base class `Invariant` requires it to be invariant


error[invalid-generic-class]: Variance of type variable `T_contra` is incompatible with base class `Covariant`
  --> src/mdtest_snippet.py:24:20
   |
24 | class BadCovariant(Covariant[T_contra]): ...
   |                    ^^^^^^^^^^^^^^^^^^^
help: Type variable `T_contra` is declared as contravariant, but base class `Covariant` requires it to be covariant


error[invalid-generic-class]: Variance of type variable `T_co` is incompatible with base class `Contravariant`
  --> src/mdtest_snippet.py:27:24
   |
27 | class BadContravariant(Contravariant[T_co]): ...
   |                        ^^^^^^^^^^^^^^^^^^^
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
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to
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

## Inferred variance for writable subclass-type attributes

A writable public `type[T]` attribute makes a legacy type variable with inferred variance invariant.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Generic, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

T = TypeVar("T", infer_variance=True)

class ClassContainer(Generic[T]):
    cls: type[T]

static_assert(not is_subtype_of(ClassContainer[int], ClassContainer[object]))
static_assert(not is_subtype_of(ClassContainer[object], ClassContainer[int]))

static_assert(not is_assignable_to(ClassContainer[int], ClassContainer[object]))
static_assert(not is_assignable_to(ClassContainer[object], ClassContainer[int]))
```

## Inferred variance for subclass-type method parameters

A method parameter annotated as `type[T]` makes a legacy type variable with inferred variance
contravariant.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Generic, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

T = TypeVar("T", infer_variance=True)

class ClassContainer(Generic[T]):
    def put(self, cls: type[T]) -> None: ...

static_assert(is_subtype_of(ClassContainer[object], ClassContainer[int]))
static_assert(not is_subtype_of(ClassContainer[int], ClassContainer[object]))

static_assert(is_assignable_to(ClassContainer[object], ClassContainer[int]))
static_assert(not is_assignable_to(ClassContainer[int], ClassContainer[object]))
```

[spec]: https://typing.python.org/en/latest/spec/generics.html#variance
