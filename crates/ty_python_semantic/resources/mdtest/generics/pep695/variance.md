# Variance: PEP 695 syntax

```toml
[environment]
python-version = "3.12"
```

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
from typing import Any

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
from typing import Any

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
from typing import Any

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

This is a bit of pathological case, which really only happens when the class doesn't use the typevar
at all. (If it did, it would have to be covariant, contravariant, or invariant, depending on _how_
the typevar was used.)

```py
from ty_extensions import is_assignable_to, is_equivalent_to, is_subtype_of, static_assert, Unknown
from typing import Any

class A: ...
class B(A): ...

class C[T]:
    pass

class D[U](C[U]):
    pass

static_assert(is_assignable_to(C[B], C[A]))
static_assert(is_assignable_to(C[A], C[B]))
static_assert(is_assignable_to(C[A], C[Any]))
static_assert(is_assignable_to(C[B], C[Any]))
static_assert(is_assignable_to(C[Any], C[A]))
static_assert(is_assignable_to(C[Any], C[B]))

static_assert(is_assignable_to(D[B], C[A]))
static_assert(is_subtype_of(C[A], C[A]))
static_assert(is_assignable_to(D[A], C[B]))
static_assert(is_assignable_to(D[A], C[Any]))
static_assert(is_assignable_to(D[B], C[Any]))
static_assert(is_assignable_to(D[Any], C[A]))
static_assert(is_assignable_to(D[Any], C[B]))

static_assert(is_subtype_of(C[B], C[A]))
static_assert(is_subtype_of(C[A], C[B]))
static_assert(not is_subtype_of(C[A], C[Any]))
static_assert(not is_subtype_of(C[B], C[Any]))
static_assert(not is_subtype_of(C[Any], C[A]))
static_assert(not is_subtype_of(C[Any], C[B]))
static_assert(not is_subtype_of(C[Any], C[Any]))

static_assert(is_subtype_of(D[B], C[A]))
static_assert(is_subtype_of(D[A], C[B]))
static_assert(not is_subtype_of(D[A], C[Any]))
static_assert(not is_subtype_of(D[B], C[Any]))
static_assert(not is_subtype_of(D[Any], C[A]))
static_assert(not is_subtype_of(D[Any], C[B]))

static_assert(is_equivalent_to(C[A], C[A]))
static_assert(is_equivalent_to(C[B], C[B]))
static_assert(is_equivalent_to(C[B], C[A]))
static_assert(is_equivalent_to(C[A], C[B]))
static_assert(is_equivalent_to(C[A], C[Any]))
static_assert(is_equivalent_to(C[B], C[Any]))
static_assert(is_equivalent_to(C[Any], C[A]))
static_assert(is_equivalent_to(C[Any], C[B]))

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

## Mutual Recursion

This example due to Martin Huschenbett's PyCon 2025 talk,
[Linear Time variance Inference for PEP 695][linear-time-variance-talk]

```py
from ty_extensions import is_subtype_of, static_assert
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

We want the _greatest_ inferrable variance, so we start by assuming `X` occurs bivariantly in `C`.
Therefore, `X` has one bivariant occurrence, and one contravariant occurrence, which resolves to
contravariant.

Recall that `Y`'s only occurrence in `D` is covariantly in `C[Y]` (`Y` subbing in for `X`). We just
stated that `X` is contravariant in `C`, so we conclude that `Y` is contravariant in `D`.

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

Normal attributes are mutable, and so make the enclosing class invariant in this typevar (see [inv]).

```py
from ty_extensions import is_subtype_of, static_assert

class A: ...
class B(A): ...

class C[T]:
    x: T

static_assert(not is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
```

One might think that normal attributes are

### Immutable Attributes

Immutable attributes can't be written to, and thus constrain the typevar to covariance, not
invariance:

```py
from dataclasses import dataclass
from typing import Final, NamedTuple
from ty_extensions import is_subtype_of, static_assert

class A: ...
class B(A): ...

class C[T]:
    x: Final[T]

@dataclass(frozen=True)
class D[U]:
    y: U

class E[V](NamedTuple):
    z: V

static_assert(is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))

static_assert(is_subtype_of(D[B], D[A]))
static_assert(not is_subtype_of(D[A], D[B]))

static_assert(is_subtype_of(E[B], E[A]))
static_assert(not is_subtype_of(E[A], E[B]))
```

### Properties

Properties constrain to covariance if they are get-only and invariant if they are get-set:

```py
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
```

while accessing `D().x` will give you an `int`, the descriptor methods say this should be
contravariant

```py
from ty_extensions import static_assert, is_subtype_of

static_assert(is_subtype_of(C[B], C[A]))
static_assert(not is_subtype_of(C[A], C[B]))
static_assert(not is_subtype_of(D[B], D[A]))
static_assert(not is_subtype_of(D[A], D[B]))
```

### Implicit Attributes

Implicit attributes work like normal ones

```py
from ty_extensions import static_assert, is_subtype_of

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
from ty_extensions import static_assert, is_subtype_of

class A: ...
class B(A): ...

class C[T]:
    def __init__(self, x: T): ...
    def __new__(self, x: T): ...

static_assert(is_subtype_of(C[B], C[A]))
static_assert(is_subtype_of(C[A], C[B]))
```

This example is then bivariant because it doesn't use `T` outside of the two exempted methods.

This holds likewise for dataclasses with synthesized `__init__`:

```py
from dataclasses import dataclass

@dataclass(init=True)
class D[T]: ...

static_assert(is_subtype_of(D[B], D[A]))
static_assert(is_subtype_of(D[A], D[B]))
```

## Union Types

Union types are covariant in all their members. If `A <: B`, then `A | C <: B | C` and
`C | A <: C | B`.

```py
from ty_extensions import is_assignable_to, is_subtype_of, static_assert

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
from ty_extensions import is_assignable_to, is_subtype_of, static_assert, Intersection, Not

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
in `T` because if `A <: B`, then `type[A] <: type[B]` holds.

```py
from ty_extensions import is_assignable_to, is_subtype_of, static_assert

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
        self.cls = cls

    def create_instance(self) -> T:
        return self.cls()

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

[linear-time-variance-talk]: https://www.youtube.com/watch?v=7uixlNTOY4s&t=9705s
[spec]: https://typing.python.org/en/latest/spec/generics.html#variance
