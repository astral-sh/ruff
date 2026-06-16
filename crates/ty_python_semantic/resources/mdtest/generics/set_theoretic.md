# Generics and set theoretic types

This test suite explores the interplay between generics and set theoretic gradual types.

```toml
[environment]
python-version = "3.14"
```

```pyi
from typing import Any
from ty_extensions import static_assert, is_equivalent_to, is_subtype_of
```

Throughout the document, we use the following classes as canonical examples for covariant,
contravariant, and invariant generic classes:

```pyi
class Co[T]:
    def get(self) -> T:
        raise NotImplementedError

class Contra[T]:
    def push(self, x: T) -> None: ...

class Invariant[T](Co[T], Contra[T]): ...
```

Further, we use `P` and `Q` as placeholders for arbitrary fully-static (non-generic types) that are
not related in any way, and we use `Base` and `Sub` as examples for types that are in a subtyping
relationship `Sub <: Base`:

```pyi
# Two unrelated placeholder types:
class P: ...
class Q: ...

# Two types with a subtyping relationship:
class Base: ...
class Sub(Base): ...
```

We start by demonstrating that `Co`, `Contra`, and `Invariant` are indeed what they claim to be:

```pyi
static_assert(is_subtype_of(Co[Sub], Co[Base]))
static_assert(not is_subtype_of(Co[Base], Co[Sub]))

static_assert(not is_subtype_of(Contra[Sub], Contra[Base]))
static_assert(is_subtype_of(Contra[Base], Contra[Sub]))

static_assert(not is_subtype_of(Invariant[Sub], Invariant[Base]))
static_assert(not is_subtype_of(Invariant[Base], Invariant[Sub]))
```

We now want to look at the interplay between unions and intersections on the one hand, and the
different kinds of generic classes on the other hand. The following relations follow immediately
from the subtyping behavior of the respective "lifted" subtyping relations of `Co` and `Contra`:

```ignore
Co[Base] | Co[Sub] = Co[Base]
Co[Base] & Co[Sub] = Co[Sub]

Contra[Base] | Contra[Sub] = Contra[Sub]
Contra[Base] & Contra[Sub] = Contra[Base]
```

We can encode these in ty assertions:

```pyi
static_assert(is_equivalent_to(Co[Base] | Co[Sub], Co[Base]))
static_assert(is_equivalent_to(Co[Base] & Co[Sub], Co[Sub]))

static_assert(is_equivalent_to(Contra[Base] | Contra[Sub], Contra[Sub]))
static_assert(is_equivalent_to(Contra[Base] & Contra[Sub], Contra[Base]))
```

For invariant generics, neither of those relations are true:

```pyi
static_assert(not is_equivalent_to(Invariant[Base] | Invariant[Sub], Invariant[Base]))
static_assert(not is_equivalent_to(Invariant[Base] | Invariant[Sub], Invariant[Sub]))

static_assert(not is_equivalent_to(Invariant[Base] & Invariant[Sub], Invariant[Base]))
static_assert(not is_equivalent_to(Invariant[Base] & Invariant[Sub], Invariant[Sub]))
```

When there is no subtyping relationship between the specializations of these generic classes, we
generally don't have equality relations, but we can still derive the following inequalities by
observing that both `Co[P]` and `Co[Q]` are subtypes of `Co[P | Q]`, and so their union is also
subtype of `Co[P | Q]`. Similarly, since both `Co[P]` and `Co[Q]` are supertypes of `Co[P & Q]`,
then so is their intersection:

```ignore
Co[P] | Co[Q] <: Co[P | Q]    (1a)
Co[P] & Co[Q] :> Co[P & Q]    (1b)
```

Translated into ty assertions, we have:

```pyi
static_assert(is_subtype_of(Co[P] | Co[Q], Co[P | Q]))
static_assert(not is_equivalent_to(Co[P] | Co[Q], Co[P | Q]))

static_assert(is_subtype_of(Co[P & Q], Co[P] & Co[Q]))
static_assert(not is_equivalent_to(Co[P & Q], Co[P] & Co[Q]))
```

In a similar way, we can see that the following relations hold for *contravariant* generic types.
Note that on the right hand side, unions have turned into intersections, and vice versa:

```ignore
Contra[P] | Contra[Q] <: Contra[P & Q]    (2a)
Contra[P] & Contra[Q] :> Contra[P | Q]    (2b)
```

And again, we can verify those in ty:

```pyi
static_assert(is_subtype_of(Contra[P] | Contra[Q], Contra[P & Q]))
static_assert(not is_equivalent_to(Contra[P] | Contra[Q], Contra[P & Q]))

static_assert(is_subtype_of(Contra[P | Q], Contra[P] & Contra[Q]))
static_assert(not is_equivalent_to(Contra[P | Q], Contra[P] & Contra[Q]))
```

Next, we want to explore specializations with dynamic types. In general, we can express a gradual
type in its canonical interval representation `G = Bottom[G] | Top[G] & Any`. In
top-materializations, we replace covariant/contravariant uses of dynamic types with
`object`/`Never`. In bottom-materializations, we do the opposite. Therefore, we get:

```ignore
    Co[Any] = Co[Never]      | Co[object]    & Any    (3)
Contra[Any] = Contra[object] | Contra[Never] & Any    (4)
```

These representations lead to interesting simplifications in unions and intersections. For example,
we can transform `Co[P] | Co[Any]` in the following way:

```ignore
Co[P] | Co[Any] = Co[P] | Co[Never] | Co[object] & Any
                = Co[P] | Co[object] & Any
                = Co[P | Any]
```

The last step in this derivation is better followed in reverse. The second-to-last line is just the
canonical interval representation of `Co[P | Any]`. For intersections, we need to do slightly more
work to arrive at a structurally similar result:

```ignore
Co[P] & Co[Any] = Co[P] & (Co[Never] | Co[object] & Any)
                = Co[P] & Co[Never] | Co[P] & Co[object] & Any
                = Co[Never] | Co[P] & Any
                = Co[P & Any]
```

For contravariant types, we get similar relations, where unions and intersections swap places again:

```ignore
Contra[P] | Contra[Any] = Contra[P] | Contra[object] | Contra[Never] & Any
                        = Contra[P] | Contra[Never] & Any
                        = Contra[P & Any]

Contra[P] & Contra[Any] = Contra[P] & (Contra[object] | Contra[Never] & Any)
                        = Contra[P] & Contra[object] | Contra[P] & Contra[Never] & Any
                        = Contra[object] | Contra[P] & Any
                        = Contra[P | Any]
```

In summary, we have:

```ignore
Co[P] | Co[Any] = Co[P | Any]                (4a)
Co[P] & Co[Any] = Co[P & Any]                (4b)

Contra[P] | Contra[Any] = Contra[P & Any]    (5a)
Contra[P] & Contra[Any] = Contra[P | Any]    (5b)
```

We can encode all of these in ty assertions:

```pyi
# TODO: all of these should pass
static_assert(is_equivalent_to(Co[P] | Co[Any], Co[P | Any]))  # error: [static-assert-error]
static_assert(is_equivalent_to(Co[P] & Co[Any], Co[P & Any]))  # error: [static-assert-error]

static_assert(is_equivalent_to(Contra[P] | Contra[Any], Contra[P & Any]))  # error: [static-assert-error]
static_assert(is_equivalent_to(Contra[P] & Contra[Any], Contra[P | Any]))  # error: [static-assert-error]
```

What about invariance? We can naively write `Invariant[Any]` in its interval representation:

```ignore
Invariant[Any] = Bottom[Invariant[Any]] | Top[Invariant[Any]] & Any
```

It's currently an open question if that representation is correct, though. `Bottom[Invariant[Any]]`
*should* represent the infinite intersection of all possible `Invariant`-specializations, and that
*should* simplify to `Never` because `Invariant[P]` and `Invariant[Q]` have no common inhabitant. On
the other hand, that would mean that `Invariant[Any]` is equivalent to `Top[Invariant[Any]] & Any`,
which is a gradual type that extends all the way down to `Never`, allowing arbitrary attributes to
be accessed on that type, which seems undesirable. Most users probably interpret `Invariant[Any]` as
"an instance of `Invariant` with an unknown specialization", but it's possible that this is not
compatible with the view that gradual types are intervals in the lattice of fully-static types. One
possible way out could be to interpret `Bottom[Invariant[Any]]` as a special new bottom type that
still represents the essence of what it means to be an instance of `Invariant`, even if that type
itself wouldn't have any inhabitants. That, however, poses the question if
`Invariant[P] & Invariant[Q]` should also resolve to that special bottom type. In any case, we
certainly have `Bottom[Invariant[Any]] <: Invariant[P]` and `Invariant[P] <: Top[Invariant[Any]]`,
and so we can simplify the following union:

```ignore
Invariant[P] | Invariant[Any]
    = Invariant[P] | Bottom[Invariant[Any]] | Top[Invariant[Any]] & Any
    = Invariant[P] | Top[Invariant[Any]] & Any
```

And for intersections, we get:

```ignore
Invariant[P] & Invariant[Any]
    = Invariant[P] & (Bottom[Invariant[Any]] | Top[Invariant[Any]] & Any)
    = Bottom[Invariant[Any]] | Invariant[P] & Any
```

If we use the interpretation where `Bottom[Invariant[Any]]` is a special bottom type that captures
"being an instance of `Invariant`", then we can see that this last line simplifies to
`Invariant[P]`, beause there is no (true) subtype of `Invariant[P]` that is also an instance of
`Invariant`. And so we get:

```ignore
Invariant[P] & Invariant[Any] = Invariant[P]
```

If we compare this to the covariant and contravariant versions (4b) and (5b), we see that
"combining" `P & Any` (the interval from `Never` to `P`) and `P | Any` (the interval from `P` to
`object`) only leaves `P` in the invariant version:

```ignore
    Co[P] & Co[Any]     =     Co[P & Any]
Contra[P] & Contra[Any] = Contra[P | Any]
```

Interestingly, the same is not true for unions. The difference is that there *are* non-trivial
supertypes of `Invariant[P]`, like `Invariant[P] | Invariant[Q]`, which are possible
materializations of the gradual type `Invariant[P] | Invariant[Any]`. So we encode two findings:

```pyi
# TODO: this should pass
static_assert(is_equivalent_to(Invariant[P] & Invariant[Any], Invariant[P]))  # error: [static-assert-error]

static_assert(not is_equivalent_to(Invariant[P] | Invariant[Any], Invariant[P]))
```
