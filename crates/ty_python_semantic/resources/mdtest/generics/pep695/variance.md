# Variance: PEP 695 syntax

```toml
[environment]
python-version = "3.12"
```

Type variables have a property called _variance_ that affects the subtyping and assignability
relations. Much more detail can be found in the [spec]. PEP 695 defines inferred variance as
**covariant**, **contravariant**, or **invariant**. We also represent **bivariance** internally, for
cases where varying a type parameter does not change the type. For PEP 695 parameters, we report
these cases as covariant, matching the spec's inference algorithm when assignment is valid in both
directions.

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
from typing import Any, Never

class A: ...
class B(A): ...

class C[T]:
    def receive(self) -> T:
        raise ValueError

class D[U](C[U]):
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
static_assert(not is_subtype_of(C[A], C[B]))
static_assert(not is_subtype_of(C[A], C[Any]))
static_assert(not is_subtype_of(C[B], C[Any]))
static_assert(not is_subtype_of(C[Any], C[A]))
static_assert(not is_subtype_of(C[Any], C[B]))
static_assert(is_subtype_of(C[Any], C[object]))
static_assert(is_subtype_of(C[Never], C[Any]))

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
from typing import Any, Never

class A: ...
class B(A): ...

class C[T]:
    def send(self, value: T): ...

class D[U](C[U]):
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
static_assert(is_subtype_of(C[object], C[Any]))
static_assert(is_subtype_of(C[Any], C[Never]))

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
class Contra[T]:
    def append(self, x: T): ...

def f[T: int](x: Contra[T]) -> T:
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
from typing import Any, Never

class A: ...
class B(A): ...

class C[T]:
    def send(self, value: T): ...
    def receive(self) -> T:
        raise ValueError

class D[U](C[U]):
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
static_assert(not is_subtype_of(C[object], C[Any]))
static_assert(not is_subtype_of(C[Any], C[Never]))

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

## Bivariant Fallback

If inference for a PEP 695 type parameter would otherwise conclude bivariance because the type
parameter is unused, we fall back to covariance instead.

```py
from ty_extensions import static_assert
from ty_extensions._internal import Unknown, is_assignable_to, is_equivalent_to, is_subtype_of
from typing import Any, Never

class A: ...
class B(A): ...

class C[T]:
    pass

class D[U](C[U]):
    pass

static_assert(is_assignable_to(C[B], C[A]))
static_assert(not is_assignable_to(C[A], C[B]))
static_assert(is_assignable_to(C[A], C[Any]))
static_assert(is_assignable_to(C[B], C[Any]))
static_assert(is_assignable_to(C[Any], C[A]))
static_assert(is_assignable_to(C[Any], C[B]))

static_assert(is_assignable_to(D[B], C[A]))
static_assert(is_subtype_of(C[A], C[A]))
static_assert(not is_assignable_to(D[A], C[B]))
static_assert(is_assignable_to(D[A], C[Any]))
static_assert(is_assignable_to(D[B], C[Any]))
static_assert(is_assignable_to(D[Any], C[A]))
static_assert(is_assignable_to(D[Any], C[B]))

static_assert(is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
static_assert(not is_subtype_of(C[A], C[Any]))
static_assert(not is_subtype_of(C[B], C[Any]))
static_assert(not is_subtype_of(C[Any], C[A]))
static_assert(not is_subtype_of(C[Any], C[B]))
static_assert(not is_subtype_of(C[Any], C[Any]))
static_assert(not is_subtype_of(C[object], C[Any]))
static_assert(not is_subtype_of(C[Any], C[Never]))

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

## Only specialized types of generic class instances influence variance

```toml
[environment]
python-version = "3.14"
```

If a generic class definition refers to a specialized instance of itself, only the specialized types
of that instance affect its variance.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class WouldBeBivariant[T]:
    def takes_int_self(self, value: WouldBeBivariant[int]): ...

static_assert(is_subtype_of(WouldBeBivariant[int], WouldBeBivariant[object]))
static_assert(not is_subtype_of(WouldBeBivariant[object], WouldBeBivariant[int]))

class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def takes_int_self(self, value: Covariant[int]): ...

static_assert(is_subtype_of(Covariant[int], Covariant[object]))
static_assert(not is_subtype_of(Covariant[object], Covariant[int]))

class Contravariant[T]:
    def send(self, value: T): ...
    def takes_int_self(self, value: Contravariant[int]): ...

static_assert(is_subtype_of(Contravariant[object], Contravariant[int]))
static_assert(not is_subtype_of(Contravariant[int], Contravariant[object]))
```

```py
class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def add[S](self: Covariant[S], other: list[S]) -> Covariant[S]:
        raise NotImplementedError

static_assert(is_subtype_of(Covariant[int], Covariant[object]))
static_assert(not is_subtype_of(Covariant[object], Covariant[int]))
```

## Nested nonrecursive protocols

Using a generic protocol inside another specialization of the same protocol is not a recursive
definition, including through a type alias. The nested `Reader` specializations do not prevent
structural variance inference for `Source`: its writable `_value` attribute makes it invariant.
Returning `Source[T]` from a nominal wrapper preserves that invariance.

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Reader[T](Protocol):
    def read(self) -> T: ...

type NestedReader[T] = Reader[Reader[T]]

class Source[T](Protocol):
    _value: T

    def reader(self) -> NestedReader[T]: ...

class Wrapper[T]:
    def source(self) -> Source[T]:
        raise NotImplementedError

static_assert(not is_subtype_of(Wrapper[int], Wrapper[object]))
static_assert(not is_subtype_of(Wrapper[object], Wrapper[int]))
```

## Recursive protocol variance

A recursive protocol that only produces its type parameter is covariant. Returning that protocol
from a nominal class preserves covariance.

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Reader[T](Protocol):
    def read(self) -> T: ...
    def next(self) -> "Reader[T]": ...

class Source[T]:
    def reader(self) -> Reader[T]:
        raise NotImplementedError

static_assert(is_subtype_of(Source[int], Source[object]))
static_assert(not is_subtype_of(Source[object], Source[int]))
```

A recursive protocol that consumes its type parameter is contravariant, even when it also returns
another instance of itself.

```py
class Writer[T](Protocol):
    def write(self, value: T) -> None: ...
    def next(self) -> "Writer[T]": ...

class Sink[T]:
    def writer(self) -> Writer[T]:
        raise NotImplementedError

static_assert(is_subtype_of(Sink[object], Sink[int]))
static_assert(not is_subtype_of(Sink[int], Sink[object]))
```

Writable attributes make recursive protocols invariant, including underscore-prefixed attributes.

```py
class Writable[T](Protocol):
    _value: T

    def next(self) -> "Writable[T]": ...

class Wrapper[T]:
    def value(self) -> Writable[T]:
        raise NotImplementedError

static_assert(not is_subtype_of(Wrapper[int], Wrapper[object]))
static_assert(not is_subtype_of(Wrapper[object], Wrapper[int]))
```

## Recursive protocol variance with annotated receivers

An explicit receiver annotation does not make a bound method consume its type parameter. `Reader`
remains covariant when its interface also recurses through the return type of `next`, and returning
that protocol from `Source` preserves covariance.

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Reader[T](Protocol):
    def read(self: "Reader[T]") -> T: ...
    def next(self) -> "Reader[T]": ...

class Source[T]:
    def reader(self) -> Reader[T]:
        raise NotImplementedError

static_assert(is_subtype_of(Source[int], Source[object]))
static_assert(not is_subtype_of(Source[object], Source[int]))
```

## Expanding recursive protocol variance

Variance inference terminates when a recursive reference changes the specialization. The mutable
list in `next` makes `Node` invariant, even though `read` produces `T` directly.

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Node[T](Protocol):
    def read(self) -> T: ...
    def next(self) -> "Node[list[T]]": ...

class Wrapper[T]:
    def node(self) -> Node[T]:
        raise NotImplementedError

static_assert(not is_subtype_of(Wrapper[int], Wrapper[object]))
static_assert(not is_subtype_of(Wrapper[object], Wrapper[int]))
```

## Mutually recursive protocol variance

A writable attribute constrains every protocol in a recursive cycle. Here, `Left` is invariant
because it returns a `Right` whose `_value` attribute can be mutated.

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Left[T](Protocol):
    def right(self) -> "Right[T]": ...

class Right[T](Protocol):
    _value: T

    def left(self) -> Left[T]: ...

class Wrapper[T]:
    def left(self) -> Left[T]:
        raise NotImplementedError

static_assert(not is_subtype_of(Wrapper[int], Wrapper[object]))
static_assert(not is_subtype_of(Wrapper[object], Wrapper[int]))
```

## Mutual Recursion

This example due to Martin Huschenbett's PyCon 2025 talk,
[Linear Time variance Inference for PEP 695][linear-time-variance-talk]

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of
from typing import Any

class A: ...
class B(A): ...

class C[X]:
    def f(self) -> "D[X]":
        return D()

    def g(self, x: X) -> None: ...

class D[Y]:
    def h(self) -> C[Y]:
        return C()
```

`C` is contravariant in `X`, and `D` in `Y`:

- `C` has two occurrences of `X`
    - `X` occurs in the return type of `f` as `D[X]` (`X` is substituted in for `Y`)
        - `D` has one occurrence of `Y`
            - `Y` occurs in the return type of `h` as `C[Y]`
    - `X` occurs contravariantly as a parameter in `g`

Thus the variance of `X` in `C` depends on itself. We want to infer the least restrictive possible
variance, so in such cases we begin by assuming that the point where we detect the cycle is
bivariant.

If we thus assume `X` is bivariant in `C`, then `Y` will be bivariant in `D`, as `D`'s only
occurrence of `Y` is in `C`. Then we consider `X` in `C` once more. We have two occurrences: `D[X]`
covariantly in a return type, and `X` contravariantly in an argument type. With one bivariant and
one contravariant occurrence, we update our inference of `X` in `C` to contravariant---the supremum
of contravariant and bivariant in the lattice.

Now that we've updated the variance of `X` in `C`, we re-evaluate `Y` in `D`. It only has the one
occurrence `C[Y]`, which we now infer is contravariant, and so we infer contravariance for `Y` in
`D` as well.

Because the variance of `X` in `C` depends on that of `Y` in `D`, we have to re-evaluate now that
we've updated the latter to contravariant. The variance of `X` in `C` is now the supremum of
contravariant and contravariant---giving us contravariant---and so remains unchanged.

Once we've completed a turn around the cycle with nothing changed, we've reached a fixed-point---the
variance inference will not change any further---and so we finally conclude that both `X` in `C` and
`Y` in `D` are contravariant.

```py
static_assert(not is_subtype_of(C[B], C[A]))
static_assert(is_subtype_of(C[A], C[B]))
static_assert(not is_subtype_of(C[A], C[Any]))
static_assert(not is_subtype_of(C[B], C[Any]))
static_assert(not is_subtype_of(C[Any], C[A]))
static_assert(not is_subtype_of(C[Any], C[B]))

static_assert(not is_subtype_of(D[B], D[A]))
static_assert(is_subtype_of(D[A], D[B]))
static_assert(not is_subtype_of(D[A], D[Any]))
static_assert(not is_subtype_of(D[B], D[Any]))
static_assert(not is_subtype_of(D[Any], D[A]))
static_assert(not is_subtype_of(D[Any], D[B]))
```

## Class Attributes

### Mutable Attributes

Normal attributes are mutable, and so make the enclosing class invariant in this typevar (see
[inv]).

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A: ...
class B(A): ...

class C[T]:
    x: T

static_assert(not is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
```

One might think that occurrences in the types of normal attributes are covariant, but they are
mutable, and thus the occurrences are invariant.

### Slotted Attributes

Slots store mutable instance attributes, so a slotted attribute also makes its type parameter
invariant.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A: ...
class B(A): ...

class Slotted[T]:
    __slots__ = ("value",)
    value: T

static_assert(not is_subtype_of(Slotted[B], Slotted[A]))
static_assert(not is_subtype_of(Slotted[A], Slotted[B]))
```

A slot descriptor also carries its mutable value type when stored directly on another generic class.
Its owner is therefore invariant even though the descriptor is assigned as a class member.

```py
class DescriptorOwner[T]:
    descriptor = Slotted[T].value

static_assert(not is_subtype_of(DescriptorOwner[B], DescriptorOwner[A]))
static_assert(not is_subtype_of(DescriptorOwner[A], DescriptorOwner[B]))
```

### Mutable protocol attributes

Underscore-prefixed protocol attributes remain writable through their structural interface, so their
inferred type parameters are invariant.

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class WritableProtocol[T](Protocol):
    _value: T

static_assert(not is_subtype_of(WritableProtocol[int], WritableProtocol[object]))
static_assert(not is_assignable_to(WritableProtocol[int], WritableProtocol[object]))

def overwrite(value: WritableProtocol[object]) -> None:
    value._value = object()

def unsound(value: WritableProtocol[int]) -> None:
    overwrite(value)  # error: [invalid-argument-type]
```

### Mutable protocol attributes with unrelated protocol members

An unrelated protocol in a member type does not change the invariance of a writable attribute. A
class that returns this protocol is also invariant, preventing callers from mutating `_value`
through a wider specialization.

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class Marker(Protocol):
    def ready(self) -> bool: ...

class WritableProtocol[T](Protocol):
    _value: T

    def marker(self) -> Marker: ...

class Wrapper[T]:
    def value(self) -> WritableProtocol[T]:
        raise NotImplementedError

static_assert(not is_subtype_of(Wrapper[int], Wrapper[object]))
static_assert(not is_assignable_to(Wrapper[int], Wrapper[object]))
```

### Immutable Attributes

Immutable attributes can't be written to, and thus constrain the typevar to covariance, not
invariance.

#### Final attributes

```py
from typing import Final
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A: ...
class B(A): ...

class C[T]:
    x: Final[T]  # error: [final-without-value]

static_assert(is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
```

#### Final attributes in stubs

Stub attributes declared as `Final` are read-only, whether their declarations omit an initializer or
use an ellipsis placeholder. A type parameter used only in such an attribute is covariant, while one
used in an ordinary writable attribute is invariant.

`box.pyi`:

```pyi
from typing import Final

class Box[T]:
    value: Final[T]

class BoxWithPlaceholder[T]:
    value: Final[T] = ...

class MutableBox[T]:
    value: T
```

`main.py`:

```py
from box import Box, BoxWithPlaceholder, MutableBox
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

static_assert(is_subtype_of(Box[int], Box[object]))
static_assert(not is_subtype_of(Box[object], Box[int]))

static_assert(is_subtype_of(BoxWithPlaceholder[int], BoxWithPlaceholder[object]))
static_assert(not is_subtype_of(BoxWithPlaceholder[object], BoxWithPlaceholder[int]))

static_assert(not is_subtype_of(MutableBox[int], MutableBox[object]))
static_assert(not is_subtype_of(MutableBox[object], MutableBox[int]))
```

#### Underscore-prefixed attributes

Underscore-prefixed instance attributes are considered private, and thus are assumed not externally
mutated.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A: ...
class B(A): ...

class C[T]:
    _x: T

    @property
    def x(self) -> T:
        return self._x

static_assert(is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))

class D[T]:
    def __init__(self, x: T):
        self._x = x

    @property
    def x(self) -> T:
        return self._x

static_assert(is_subtype_of(D[B], D[A]))
static_assert(not is_subtype_of(D[A], D[B]))
```

#### Frozen dataclasses in Python 3.12 and earlier

```py
from dataclasses import dataclass, field
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A: ...
class B(A): ...

@dataclass(frozen=True)
class D[U]:
    y: U

static_assert(is_subtype_of(D[B], D[A]))
static_assert(not is_subtype_of(D[A], D[B]))

@dataclass(frozen=True)
class E[U]:
    y: U = field()

static_assert(is_subtype_of(E[B], E[A]))
static_assert(not is_subtype_of(E[A], E[B]))
```

This also works for dataclass-transformers:

```py
from typing import dataclass_transform

@dataclass_transform(frozen_default=False)
class ModelBase:
    def __init_subclass__(cls, frozen: bool = False) -> None:
        pass

class NonFrozenModel[T](ModelBase):
    value: T

static_assert(not is_subtype_of(NonFrozenModel[B], NonFrozenModel[A]))
static_assert(not is_subtype_of(NonFrozenModel[A], NonFrozenModel[B]))

class FrozenModel[T](ModelBase, frozen=True):
    value: T

static_assert(is_subtype_of(FrozenModel[B], FrozenModel[A]))
static_assert(not is_subtype_of(FrozenModel[A], FrozenModel[B]))
```

#### Frozen dataclasses in Python 3.13 and later

```toml
[environment]
python-version = "3.13"
```

Python 3.13 introduced a new synthesized `__replace__` method on dataclasses, which uses every field
type in a contravariant position (as a parameter to `__replace__`). This means that frozen
dataclasses on Python 3.13+ can't be covariant in their field types.

```py
from dataclasses import dataclass
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A: ...
class B(A): ...

@dataclass(frozen=True)
class D[U]:
    y: U

static_assert(not is_subtype_of(D[B], D[A]))
static_assert(not is_subtype_of(D[A], D[B]))
```

The same holds for dataclass-transformers:

```py
from typing import dataclass_transform

@dataclass_transform(frozen_default=True)
class ModelBase:
    def __init_subclass__(cls, frozen: bool = True) -> None:
        pass

class DefaultFrozenModel[T](ModelBase):
    value: T

static_assert(not is_subtype_of(DefaultFrozenModel[B], DefaultFrozenModel[A]))
static_assert(not is_subtype_of(DefaultFrozenModel[A], DefaultFrozenModel[B]))

class ExplicitFrozenModel[T](ModelBase, frozen=True):
    value: T

static_assert(not is_subtype_of(ExplicitFrozenModel[B], ExplicitFrozenModel[A]))
static_assert(not is_subtype_of(ExplicitFrozenModel[A], ExplicitFrozenModel[B]))
```

#### NamedTuple

```py
from typing import NamedTuple
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A: ...
class B(A): ...

class E[V](NamedTuple):
    z: V

static_assert(is_subtype_of(E[B], E[A]))
static_assert(not is_subtype_of(E[A], E[B]))
```

A subclass of a `NamedTuple` can still be covariant:

```py
class D[T](E[T]):
    pass

static_assert(is_subtype_of(D[B], D[A]))
static_assert(not is_subtype_of(D[A], D[B]))
```

But adding a new generic attribute on the subclass makes it invariant (the added attribute is not a
`NamedTuple` field, and thus not immutable):

```py
class C[T](E[T]):
    w: T

static_assert(not is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
```

### Properties

Properties constrain to covariance if they are get-only and invariant if they are get-set:

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A: ...
class B(A): ...

class C[T]:
    @property
    def x(self) -> T | None:
        return None

class D[U]:
    @property
    def y(self) -> U | None:
        return None

    @y.setter
    def y(self, value: U): ...

static_assert(is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
static_assert(not is_subtype_of(D[B], D[A]))
static_assert(not is_subtype_of(D[A], D[B]))
```

### Property subclasses

A property subclass can carry mutable state in its own type parameters. That state makes the owning
class invariant even when the property's getter does not mention the type parameter.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

def get_value(obj: object) -> int:
    return 1

class CustomProperty[T](property):
    metadata: T

class Owner[T]:
    value = CustomProperty[T](get_value)

static_assert(not is_subtype_of(Owner[str], Owner[object]))
static_assert(not is_subtype_of(Owner[object], Owner[str]))

def overwrite(owner: Owner[object]) -> None:
    type(owner).value.metadata = object()

def misuse(owner: Owner[str]) -> str:
    overwrite(owner)  # error: [invalid-argument-type]
    return type(owner).value.metadata
```

### Implicit Attributes

Implicit attributes work like normal ones

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A: ...
class B(A): ...

class C[T]:
    def f(self) -> None:
        self.x: T | None = None

static_assert(not is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
```

### Constructors: excluding `__init__` and `__new__`

We consider it invalid to call `__init__` explicitly on an existing object. Likewise, `__new__` is
only used at the beginning of an object's life. As such, we don't need to worry about the variance
impact of these methods.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A: ...
class B(A): ...

class C[T]:
    def __init__(self, x: T): ...
    def __new__(self, x: T): ...

static_assert(is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
```

This example would otherwise be bivariant because it doesn't use `T` outside of the two exempted
methods, so we fall back to covariance.

This holds likewise for dataclasses with synthesized `__init__`:

```py
from dataclasses import dataclass

@dataclass(init=True, frozen=True)
class D[T]:
    x: T

# Covariant due to the read-only T-typed attribute; the `__init__` is ignored and doesn't make it
# invariant:

static_assert(is_subtype_of(D[B], D[A]))
static_assert(not is_subtype_of(D[A], D[B]))
```

## Union Types

Union types are covariant in all their members. If `A <: B`, then `A | C <: B | C` and
`C | A <: C | B`.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class A: ...
class B(A): ...
class C: ...

# Union types are covariant in their members
static_assert(is_subtype_of(B | C, A | C))
static_assert(is_subtype_of(C | B, C | A))
static_assert(not is_subtype_of(A | C, B | C))
static_assert(not is_subtype_of(C | A, C | B))

# Assignability follows the same pattern
static_assert(is_assignable_to(B | C, A | C))
static_assert(is_assignable_to(C | B, C | A))
static_assert(not is_assignable_to(A | C, B | C))
static_assert(not is_assignable_to(C | A, C | B))
```

## Intersection Types

Intersection types cannot be expressed directly in Python syntax, but they occur when type narrowing
creates constraints through control flow. In ty's representation, intersection types are covariant
in their positive conjuncts and contravariant in their negative conjuncts.

```py
from ty_extensions import static_assert, Intersection, Not
from ty_extensions._internal import is_assignable_to, is_subtype_of

class A: ...
class B(A): ...
class C: ...

# Test covariance in positive conjuncts
# If B <: A, then Intersection[X, B] <: Intersection[X, A]
static_assert(is_subtype_of(Intersection[C, B], Intersection[C, A]))
static_assert(not is_subtype_of(Intersection[C, A], Intersection[C, B]))

static_assert(is_assignable_to(Intersection[C, B], Intersection[C, A]))
static_assert(not is_assignable_to(Intersection[C, A], Intersection[C, B]))

# Test contravariance in negative conjuncts
# If B <: A, then Intersection[X, Not[A]] <: Intersection[X, Not[B]]
# (excluding supertype A is more restrictive than excluding subtype B)
static_assert(is_subtype_of(Intersection[C, Not[A]], Intersection[C, Not[B]]))
static_assert(not is_subtype_of(Intersection[C, Not[B]], Intersection[C, Not[A]]))

static_assert(is_assignable_to(Intersection[C, Not[A]], Intersection[C, Not[B]]))
static_assert(not is_assignable_to(Intersection[C, Not[B]], Intersection[C, Not[A]]))
```

## Subclass Types (type[T])

The `type[T]` construct represents the type of classes that are subclasses of `T`. It is covariant
in `T` because if `A <: B`, then `type[A] <: type[B]` holds. A public, writable `type[T]` attribute
still makes its enclosing class invariant, while a private attribute can remain covariant.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class A: ...
class B(A): ...

# type[T] is covariant in T
static_assert(is_subtype_of(type[B], type[A]))
static_assert(not is_subtype_of(type[A], type[B]))

static_assert(is_assignable_to(type[B], type[A]))
static_assert(not is_assignable_to(type[A], type[B]))

# With generic classes using type[T]
class ClassContainer[T]:
    def __init__(self, cls: type[T]) -> None:
        self._cls = cls

    def create_instance(self) -> T:
        return self._cls()

# ClassContainer is covariant in T due to type[T]
static_assert(is_subtype_of(ClassContainer[B], ClassContainer[A]))
static_assert(not is_subtype_of(ClassContainer[A], ClassContainer[B]))

static_assert(is_assignable_to(ClassContainer[B], ClassContainer[A]))
static_assert(not is_assignable_to(ClassContainer[A], ClassContainer[B]))

# Practical example: you can pass a ClassContainer[B] where ClassContainer[A] is expected
# because type[B] can safely be used where type[A] is expected
def use_a_class_container(container: ClassContainer[A]) -> A:
    return container.create_instance()

b_container = ClassContainer[B](B)
a_instance: A = use_a_class_container(b_container)  # This should work
```

## Subclass types in writable attributes

A writable public `type[T]` attribute makes its enclosing class invariant in `T`.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class ClassContainer[T]:
    cls: type[T]

static_assert(not is_subtype_of(ClassContainer[int], ClassContainer[object]))
static_assert(not is_subtype_of(ClassContainer[object], ClassContainer[int]))

static_assert(not is_assignable_to(ClassContainer[int], ClassContainer[object]))
static_assert(not is_assignable_to(ClassContainer[object], ClassContainer[int]))
```

## Subclass types in return positions

A `type[T]` return contributes covariance for `T`. Combining it with a method that accepts `T`
therefore makes the enclosing class invariant.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class ClassContainer[T]:
    def get(self) -> type[T]:
        raise NotImplementedError

    def put(self, value: T) -> None: ...

static_assert(not is_subtype_of(ClassContainer[int], ClassContainer[object]))
static_assert(not is_subtype_of(ClassContainer[object], ClassContainer[int]))

static_assert(not is_assignable_to(ClassContainer[int], ClassContainer[object]))
static_assert(not is_assignable_to(ClassContainer[object], ClassContainer[int]))
```

## Subclass types in parameter positions

A method parameter annotated as `type[T]` makes the enclosing class contravariant in `T`.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class ClassContainer[T]:
    def put(self, cls: type[T]) -> None: ...

static_assert(is_subtype_of(ClassContainer[object], ClassContainer[int]))
static_assert(not is_subtype_of(ClassContainer[int], ClassContainer[object]))

static_assert(is_assignable_to(ClassContainer[object], ClassContainer[int]))
static_assert(not is_assignable_to(ClassContainer[int], ClassContainer[object]))
```

## TypeIs

```toml
[environment]
python-version = "3.13"
```

`TypeIs[T]` is invariant in `T`. See the [typing spec][typeis-spec] for a justification.

```py
from typing import TypeIs
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class A:
    pass

class B(A):
    pass

class C[T]:
    def check(self, x: object) -> TypeIs[T]:
        # this is a bad check, but we only care about it type-checking
        return False

static_assert(not is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
static_assert(not is_assignable_to(C[B], C[A]))
static_assert(not is_assignable_to(C[A], C[B]))
```

## TypeGuard

`TypeGuard[T]` is covariant in `T`. The typing spec doesn't explicitly call this out, but it follows
from similar logic to invariance of `TypeIs` except without the negative case.

Formally, suppose we have types `A` and `B` with `B < A`. Take `x: object` to be the value that all
subsequent `TypeGuard`s are narrowing.

We can assign `p: TypeGuard[A] = q` where `q: TypeGuard[B]` because

- if `q` is `False`, then no constraints were learned on `x` before and none are now learned, so
    nothing changes
- if `q` is `True`, then we know `x: B`. From `B < A`, we conclude `x: A`.

We _cannot_ assign `p: TypeGuard[B] = q` where `q: TypeGuard[A]` because if `q` is `True`, we would
be concluding `x: B` from `x: A`, which is an unsafe downcast.

```py
from typing import TypeGuard
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class A:
    pass

class B(A):
    pass

class C[T]:
    def check(self, x: object) -> TypeGuard[T]:
        # this is a bad check, but we only care about it type-checking
        return False

static_assert(is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
static_assert(is_assignable_to(C[B], C[A]))
static_assert(not is_assignable_to(C[A], C[B]))
```

## Typed dictionaries

### Mutable items

A mutable `TypedDict` item can be read and written, so returning a `TypedDict` with an item of type
`T` makes the enclosing class invariant in `T`.

```py
from typing import TypedDict
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Item[T](TypedDict):
    value: T

class Producer[T]:
    def get(self) -> Item[T]:
        raise NotImplementedError

static_assert(not is_subtype_of(Producer[bool], Producer[int]))
static_assert(not is_subtype_of(Producer[int], Producer[bool]))
```

Optional items are still mutable, including items whose names start with an underscore.

```py
from typing import NotRequired

class OptionalItem[T](TypedDict):
    _value: NotRequired[T]

class OptionalProducer[T]:
    def get(self) -> OptionalItem[T]:
        raise NotImplementedError

static_assert(not is_subtype_of(OptionalProducer[bool], OptionalProducer[int]))
static_assert(not is_subtype_of(OptionalProducer[int], OptionalProducer[bool]))
```

### Read-only items

A read-only item is covariant in its value type. Returning this `TypedDict` makes a class covariant,
while accepting it as a method argument makes a class contravariant. An unrelated mutable item does
not affect the variance of `T`.

```py
from typing_extensions import ReadOnly, TypedDict
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Item[T](TypedDict):
    value: ReadOnly[T]
    tag: str

class Producer[T]:
    def get(self) -> Item[T]:
        raise NotImplementedError

class Consumer[T]:
    def put(self, item: Item[T]) -> None: ...

static_assert(is_subtype_of(Producer[bool], Producer[int]))
static_assert(not is_subtype_of(Producer[int], Producer[bool]))
static_assert(is_subtype_of(Consumer[int], Consumer[bool]))
static_assert(not is_subtype_of(Consumer[bool], Consumer[int]))
```

### Nested item types

Read-only items preserve the variance of their value types. A callable's argument and return types
contribute opposite variances; using the same type variable in both positions makes it invariant.

```py
from typing import Callable
from typing_extensions import ReadOnly, TypedDict
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Callback[P, R](TypedDict):
    callback: ReadOnly[Callable[[P], R]]

class Consumer[T]:
    def get(self) -> Callback[T, None]:
        raise NotImplementedError

class Transformer[T]:
    def get(self) -> Callback[T, T]:
        raise NotImplementedError

static_assert(is_subtype_of(Consumer[int], Consumer[bool]))
static_assert(not is_subtype_of(Consumer[bool], Consumer[int]))
static_assert(not is_subtype_of(Transformer[bool], Transformer[int]))
static_assert(not is_subtype_of(Transformer[int], Transformer[bool]))
```

### Inherited items

Inherited items contribute variance after applying the base class's specialization. Although the
item itself is read-only, the list it contains is mutable, making the enclosing class invariant.

```py
from typing_extensions import ReadOnly, TypedDict
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Base[T](TypedDict):
    value: ReadOnly[T]

class Derived[T](Base[list[T]]): ...

class Producer[T]:
    def get(self) -> Derived[T]:
        raise NotImplementedError

static_assert(not is_subtype_of(Producer[bool], Producer[int]))
static_assert(not is_subtype_of(Producer[int], Producer[bool]))
```

### Legacy type variables

When a `TypedDict` appears in another generic class, its legacy type variables contribute their
declared variance to the enclosing class's inferred variance, just as they do for protocols. An
invariant legacy type variable makes the enclosing consumer invariant even when the item is
read-only; a covariant legacy type variable makes the consumer contravariant even when the item is
mutable.

```py
from typing import Generic, TypeVar
from typing_extensions import ReadOnly, TypedDict
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

T_co = TypeVar("T_co", covariant=True)
T = TypeVar("T")

class InvariantItem(TypedDict, Generic[T]):
    # TODO: The variance rules specified for Protocol would suggest an error here: T is
    # declared invariant but used covariantly. The conformance suite does not specify this
    # check for TypedDicts, and other type checkers do not implement it.
    value: ReadOnly[T]

class CovariantItem(TypedDict, Generic[T_co]):
    # TODO: The variance rules specified for Protocol would suggest an error here: T_co is
    # declared covariant but used invariantly. The conformance suite does not specify this
    # check for TypedDicts, and other type checkers do not implement it.
    value: T_co

class InvariantConsumer[T]:
    def put(self, item: InvariantItem[T]) -> None: ...

class ContravariantConsumer[T]:
    def put(self, item: CovariantItem[T]) -> None: ...

static_assert(not is_subtype_of(InvariantConsumer[bool], InvariantConsumer[int]))
static_assert(not is_subtype_of(InvariantConsumer[int], InvariantConsumer[bool]))
static_assert(is_subtype_of(ContravariantConsumer[int], ContravariantConsumer[bool]))
static_assert(not is_subtype_of(ContravariantConsumer[bool], ContravariantConsumer[int]))
```

### Extra items

Extra items contribute variance just like named items, including when inherited. Mutable extra items
are invariant in their value type, while read-only extra items are covariant.

```py
from typing_extensions import ReadOnly, TypedDict
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class MutableExtras[T](TypedDict, extra_items=T): ...
class ReadOnlyExtras[T](TypedDict, extra_items=ReadOnly[T]): ...
class InheritedExtras[T](ReadOnlyExtras[T]): ...

class Producer[T]:
    def get(self) -> MutableExtras[T]:
        raise NotImplementedError

class Consumer[T]:
    def put(self, item: InheritedExtras[T]) -> None: ...

static_assert(not is_subtype_of(Producer[bool], Producer[int]))
static_assert(not is_subtype_of(Producer[int], Producer[bool]))
static_assert(is_subtype_of(Consumer[int], Consumer[bool]))
static_assert(not is_subtype_of(Consumer[bool], Consumer[int]))
```

### Functional syntax

Items defined with functional syntax can refer to an enclosing class's type parameter. The item
schema determines variance, including when it contains a recursive reference.

```py
from typing_extensions import ReadOnly, TypedDict
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Consumer[T]:
    Item = TypedDict("Item", {"child": "ReadOnly[Item | None]", "value": ReadOnly[T]})

    def put(self, item: Item) -> None: ...

static_assert(is_subtype_of(Consumer[int], Consumer[bool]))
static_assert(not is_subtype_of(Consumer[bool], Consumer[int]))
```

### Recursive items

A recursive read-only item preserves covariance when every occurrence of the type variable is
covariant. Accepting the recursive `TypedDict` as a method argument makes the class contravariant.

```py
from typing_extensions import ReadOnly, TypedDict
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Node[T](TypedDict):
    child: ReadOnly["Node[T] | None"]
    value: ReadOnly[T]

class Consumer[T]:
    def put(self, item: Node[T]) -> None: ...

static_assert(is_subtype_of(Consumer[int], Consumer[bool]))
static_assert(not is_subtype_of(Consumer[bool], Consumer[int]))
```

### Expanding recursive items

Variance inference terminates even when a recursive item wraps the type argument in another type.
Here the nested `list[T]` makes `T` invariant despite both items being read-only.

```py
from typing_extensions import ReadOnly, TypedDict
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class Node[T](TypedDict):
    child: ReadOnly["Node[list[T]] | None"]
    value: ReadOnly[T]

class Producer[T]:
    def get(self) -> Node[T]:
        raise NotImplementedError

static_assert(not is_subtype_of(Producer[bool], Producer[int]))
static_assert(not is_subtype_of(Producer[int], Producer[bool]))
```

## Type aliases

The variance of the type alias matches the variance of the value type (RHS type).

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of
from typing import Literal

class Covariant[T]:
    def get(self) -> T:
        raise ValueError

type CovariantLiteral1 = Covariant[Literal[1]]
type CovariantInt = Covariant[int]
type MyCovariant[T] = Covariant[T]

static_assert(is_subtype_of(CovariantLiteral1, CovariantInt))
static_assert(is_subtype_of(MyCovariant[Literal[1]], MyCovariant[int]))

class Contravariant[T]:
    def set(self, value: T):
        pass

type ContravariantLiteral1 = Contravariant[Literal[1]]
type ContravariantInt = Contravariant[int]
type MyContravariant[T] = Contravariant[T]

static_assert(is_subtype_of(ContravariantInt, ContravariantLiteral1))
static_assert(is_subtype_of(MyContravariant[int], MyContravariant[Literal[1]]))

class Invariant[T]:
    def get(self) -> T:
        raise ValueError

    def set(self, value: T):
        pass

type InvariantLiteral1 = Invariant[Literal[1]]
type InvariantInt = Invariant[int]
type MyInvariant[T] = Invariant[T]

static_assert(not is_subtype_of(InvariantInt, InvariantLiteral1))
static_assert(not is_subtype_of(InvariantLiteral1, InvariantInt))
static_assert(not is_subtype_of(MyInvariant[Literal[1]], MyInvariant[int]))
static_assert(not is_subtype_of(MyInvariant[int], MyInvariant[Literal[1]]))

class WouldBeBivariant[T]:
    pass

type WouldBeBivariantLiteral1 = WouldBeBivariant[Literal[1]]
type WouldBeBivariantInt = WouldBeBivariant[int]
type MyWouldBeBivariant[T] = WouldBeBivariant[T]

static_assert(not is_subtype_of(WouldBeBivariantInt, WouldBeBivariantLiteral1))
static_assert(is_subtype_of(WouldBeBivariantLiteral1, WouldBeBivariantInt))
static_assert(is_subtype_of(MyWouldBeBivariant[Literal[1]], MyWouldBeBivariant[int]))
static_assert(not is_subtype_of(MyWouldBeBivariant[int], MyWouldBeBivariant[Literal[1]]))
```

## Inheriting from generic classes with inferred variance

When inheriting from a generic class with our type variable substituted in, we count its occurrences
as well. In the following example, `T` is covariant in `C`, and contravariant in the subclass `D` if
you only count its own occurrences. Because we count both then, `T` is invariant in `D`.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class A:
    pass

class B(A):
    pass

class C[T]:
    def f() -> T | None:
        pass

static_assert(is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))

class D[T](C[T]):
    def g(x: T) -> None:
        pass

static_assert(not is_subtype_of(D[B], D[A]))
static_assert(not is_subtype_of(D[A], D[B]))
```

## Inheriting from generic classes with explicit variance

```py
from typing import TypeVar, Generic
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)
T_contra = TypeVar("T_contra", contravariant=True)

class A:
    pass

class B(A):
    pass

class Invariant(Generic[T]):
    pass

static_assert(not is_subtype_of(Invariant[B], Invariant[A]))
static_assert(not is_subtype_of(Invariant[A], Invariant[B]))

class DerivedInvariant[T](Invariant[T]):
    pass

static_assert(not is_subtype_of(DerivedInvariant[B], DerivedInvariant[A]))
static_assert(not is_subtype_of(DerivedInvariant[A], DerivedInvariant[B]))

class Covariant(Generic[T_co]):
    pass

static_assert(is_subtype_of(Covariant[B], Covariant[A]))
static_assert(not is_subtype_of(Covariant[A], Covariant[B]))

class DerivedCovariant[T](Covariant[T]):
    pass

static_assert(is_subtype_of(DerivedCovariant[B], DerivedCovariant[A]))
static_assert(not is_subtype_of(DerivedCovariant[A], DerivedCovariant[B]))

class Contravariant(Generic[T_contra]):
    pass

static_assert(not is_subtype_of(Contravariant[B], Contravariant[A]))
static_assert(is_subtype_of(Contravariant[A], Contravariant[B]))

class DerivedContravariant[T](Contravariant[T]):
    pass

static_assert(not is_subtype_of(DerivedContravariant[B], DerivedContravariant[A]))
static_assert(is_subtype_of(DerivedContravariant[A], DerivedContravariant[B]))
```

[linear-time-variance-talk]: https://www.youtube.com/watch?v=7uixlNTOY4s&t=9705s
[spec]: https://typing.python.org/en/latest/spec/generics.html#variance
[typeis-spec]: https://typing.python.org/en/latest/spec/narrowing.html#typeis
