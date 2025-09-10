# Constraints

```toml
[environment]
python-version = "3.13"
```

For "concrete" types (which contain no type variables), type properties like assignability have
simple answers: one type is either assignable to another type, or it isn't. (The _rules_ for
comparing two particular concrete types can be rather complex, but the _answer_ is a simple "yes" or
"no".)

These properties are more complex when type variables are involved, because there are (usually) many
different concrete types that a typevar can be specialized to, and the type property might hold for
some specializations, but not for others. That means that for types that include typevars, "Is this
type assignable to another?" no longer makes sense as a question. The better question is: "Under
what constraints is this type assignable to another?".

An individual constraint restricts the specialization of a single typevar. You can then build up
more complex constraint sets using union, intersection, and negation operations. We use a
disjunctive normal form (DNF) representation, just like we do for types: a _constraint set_ is the
union of zero or more _clauses_, each of which is the intersection of zero or more _individual
constraints_. Note that the constraint set that contains no clauses is never satisfiable
(`⋃ {} = 0`); and the constraint set that contains a single clause, where that clause contains no
constraints, is always satisfiable (`⋃ {⋂ {}} = 1`).

## Kinds of constraints

### Range

A _range_ constraint requires the typevar to be within a particular lower and upper bound: the
typevar can only specialize to a type that is a supertype of the lower bound, and a subtype of the
upper bound.

```py
from typing import Any, final, Never, Sequence
from ty_extensions import range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(Sub, T, Super))
```

Every type is a supertype of `Never`, so a lower bound of `Never` is the same as having no lower
bound.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≤ Base)]
    reveal_type(range_constraint(Never, T, Base))
```

Similarly, every type is a subtype of `object`, so an upper bound of `object` is the same as having
no upper bound.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_)]
    reveal_type(range_constraint(Base, T, object))
```

And a range constraint with _both_ a lower bound of `Never` and an upper bound of `object` does not
constrain the typevar at all.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(range_constraint(Never, T, object))
```

If the lower bound and upper bounds are "inverted" (the upper bound is a subtype of the lower bound)
or incomparable, then there is no type that can satisfy the constraint.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(Super, T, Sub))
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(Base, T, Unrelated))
```

The lower and upper bound can be the same type, in which case the typevar can only be specialized to
that specific type.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_ ≤ Base)]
    reveal_type(range_constraint(Base, T, Base))
```

Constraints can only refer to fully static types, so the lower and upper bounds are transformed into
their bottom and top materializations, respectively.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_)]
    reveal_type(range_constraint(Base, T, Any))
    # revealed: ty_extensions.ConstraintSet[(Sequence[Base] ≤ T@_ ≤ Sequence[object])]
    reveal_type(range_constraint(Sequence[Base], T, Sequence[Any]))

    # revealed: ty_extensions.ConstraintSet[(T@_ ≤ Base)]
    reveal_type(range_constraint(Any, T, Base))
    # revealed: ty_extensions.ConstraintSet[(Sequence[Never] ≤ T@_ ≤ Sequence[Base])]
    reveal_type(range_constraint(Sequence[Any], T, Sequence[Base]))
```

### Not equivalent

A _not-equivalent_ constraint requires the typevar to specialize to anything _other_ than a
particular type (the "hole").

```py
from typing import Any, Never, Sequence
from ty_extensions import not_equivalent_constraint

class Base: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base)]
    reveal_type(not_equivalent_constraint(T, Base))
```

Unlike range constraints, `Never` and `object` are not special when used as the hole of a
not-equivalent constraint — there are many types that are not equivalent to `Never` or `object`.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Never)]
    reveal_type(not_equivalent_constraint(T, Never))

    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ object)]
    reveal_type(not_equivalent_constraint(T, object))
```

Constraints can only refer to fully static types. However, not-equivalent constraints are not
created directly; they are only created when negating a range constraint. Since that range
constraint will have fully static lower and upper bounds, the not-equivalent constraints that we
create will already have a fully static hole. Therefore, we raise a diagnostic when calling the
internal `not_equivalent_constraint` constructor with a non-fully-static type.

```py
def _[T]() -> None:
    # error: [invalid-argument-type] "Not-equivalent constraint must have a fully static type"
    reveal_type(not_equivalent_constraint(T, Any))  # revealed: ConstraintSet
    # error: [invalid-argument-type] "Not-equivalent constraint must have a fully static type"
    reveal_type(not_equivalent_constraint(T, Sequence[Any]))  # revealed: ConstraintSet
```

### Incomparable

An _incomparable_ constraint requires the typevar to specialize to any type that is neither a
subtype nor a supertype of a particular type (the "pivot").

```py
from typing import Any, Never, Sequence
from ty_extensions import incomparable_constraint

class Base: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base)]
    reveal_type(incomparable_constraint(T, Base))
```

Every type is comparable with `Never` and with `object`, so an incomparable constraint with either
as a pivot cannot ever be satisfied.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(incomparable_constraint(T, Never))

    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(incomparable_constraint(T, object))
```

Constraints can only refer to fully static types. However, incomparable constraints are not created
directly; they are only created when negating a range constraint. Since that range constraint will
have fully static lower and upper bounds, the incomparable constraints that we create will already
have a fully static hole. Therefore, we raise a diagnostic when calling the internal
`incomparable_constraint` constructor with a non-fully-static type.

```py
def _[T]() -> None:
    # error: [invalid-argument-type] "Incomparable constraint must have a fully static type"
    reveal_type(incomparable_constraint(T, Any))  # revealed: ConstraintSet
    # error: [invalid-argument-type] "Incomparable constraint must have a fully static type"
    reveal_type(incomparable_constraint(T, Sequence[Any]))  # revealed: ConstraintSet
```

## Intersection

The intersection of two constraint sets requires that the constraints in both sets hold. In many
cases, we can simplify the result of an intersection.

### Different typevars

```py
from ty_extensions import incomparable_constraint, not_equivalent_constraint, range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...
```

We cannot simplify the intersection of constraints that refer to different typevars.

```py
def _[T, U]() -> None:
    # revealed: ty_extensions.ConstraintSet[((T@_ ≁ Base) ∧ (U@_ ≁ Base))]
    reveal_type(incomparable_constraint(T, Base) & incomparable_constraint(U, Base))
    # revealed: ty_extensions.ConstraintSet[((T@_ ≠ Base) ∧ (U@_ ≠ Base))]
    reveal_type(not_equivalent_constraint(T, Base) & not_equivalent_constraint(U, Base))
    # revealed: ty_extensions.ConstraintSet[((Sub ≤ T@_ ≤ Base) ∧ (Sub ≤ U@_ ≤ Base))]
    reveal_type(range_constraint(Sub, T, Base) & range_constraint(Sub, U, Base))
```

Intersection is reflexive.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base)]
    reveal_type(incomparable_constraint(T, Base) & incomparable_constraint(T, Base))
```

### Intersection of two ranges

The intersection of two ranges is where the ranges "overlap".

```py
from typing import final
from ty_extensions import range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class SubSub(Sub): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Base)]
    reveal_type(range_constraint(SubSub, T, Base) & range_constraint(Sub, T, Super))
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Base)]
    reveal_type(range_constraint(SubSub, T, Super) & range_constraint(Sub, T, Base))
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_ ≤ Base)]
    reveal_type(range_constraint(Sub, T, Base) & range_constraint(Base, T, Super))
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(Sub, T, Super) & range_constraint(Sub, T, Super))
```

If they don't overlap, the intersection is empty.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(SubSub, T, Sub) & range_constraint(Base, T, Super))
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(SubSub, T, Sub) & range_constraint(Unrelated, T, object))
```

### Intersection of range and not-equivalent

If the hole of a not-equivalent constraint is within the lower and upper bounds of a range
constraint, the intersection "removes" the hole from the range. The intersection cannot be
simplified.

```py
from typing import final
from ty_extensions import not_equivalent_constraint, range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[((Sub ≤ T@_ ≤ Super) ∧ (T@_ ≠ Base))]
    reveal_type(range_constraint(Sub, T, Super) & not_equivalent_constraint(T, Base))
```

If the hole is not within the lower and upper bounds (because it's a subtype of the lower bound, a
supertype of the upper bound, or not comparable with either), then removing the hole doesn't do
anything.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(Base, T, Super) & not_equivalent_constraint(T, Sub))
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(Base, T, Super) & not_equivalent_constraint(T, Unrelated))
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Base)]
    reveal_type(range_constraint(Sub, T, Base) & not_equivalent_constraint(T, Super))
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Base)]
    reveal_type(range_constraint(Sub, T, Base) & not_equivalent_constraint(T, Unrelated))
```

If the lower and upper bounds are the same, it's actually an "equivalent" constraint. If the hole is
also that same type, then the intersection is empty — the not-equivalent constraint removes the only
type that satisfies the range constraint.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(Base, T, Base) & not_equivalent_constraint(T, Base))
```

### Intersection of range and incomparable

The intersection of a range constraint and an incomparable constraint cannot be satisfied if the
pivot is a subtype of the lower bound, or a supertype of the upper bound. (If the pivot is a subtype
of the lower bound, then by transitivity, the pivot is also a subtype of everything in the range.)

```py
from typing import final
from ty_extensions import incomparable_constraint, range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(Base, T, Super) & incomparable_constraint(T, Sub))
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(Base, T, Super) & incomparable_constraint(T, Base))

    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(Sub, T, Base) & incomparable_constraint(T, Super))
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(Sub, T, Base) & incomparable_constraint(T, Base))
```

Otherwise, the intersection cannot be simplified.

```py
from ty_extensions import is_subtype_of

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[((Base ≤ T@_ ≤ Super) ∧ (T@_ ≁ Unrelated))]
    reveal_type(range_constraint(Base, T, Super) & incomparable_constraint(T, Unrelated))
```

### Intersection of two not-equivalents

Intersection is reflexive, so the intersection of a not-equivalent constraint with itself is itself.

```py
from typing import final
from ty_extensions import not_equivalent_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base)]
    reveal_type(not_equivalent_constraint(T, Base) & not_equivalent_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[((T@_ ≠ Base) ∧ (T@_ ≠ Super))]
    reveal_type(not_equivalent_constraint(T, Base) & not_equivalent_constraint(T, Super))
    # revealed: ty_extensions.ConstraintSet[((T@_ ≠ Base) ∧ (T@_ ≠ Sub))]
    reveal_type(not_equivalent_constraint(T, Base) & not_equivalent_constraint(T, Sub))
    # revealed: ty_extensions.ConstraintSet[((T@_ ≠ Base) ∧ (T@_ ≠ Unrelated))]
    reveal_type(not_equivalent_constraint(T, Base) & not_equivalent_constraint(T, Unrelated))
```

### Intersection of not-equivalent and incomparable

When intersecting a not-equivalent constraint and an incomparable constraint, if the hole and pivot
are comparable, then the incomparable constraint already excludes the hole, so removing the hole
doesn't do anything.

```py
from typing import final
from ty_extensions import incomparable_constraint, not_equivalent_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base)]
    reveal_type(not_equivalent_constraint(T, Super) & incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base)]
    reveal_type(not_equivalent_constraint(T, Base) & incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base)]
    reveal_type(not_equivalent_constraint(T, Sub) & incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[((T@_ ≠ Unrelated) ∧ (T@_ ≁ Base))]
    reveal_type(not_equivalent_constraint(T, Unrelated) & incomparable_constraint(T, Base))

    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Super)]
    reveal_type(not_equivalent_constraint(T, Base) & incomparable_constraint(T, Super))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base)]
    reveal_type(not_equivalent_constraint(T, Base) & incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Sub)]
    reveal_type(not_equivalent_constraint(T, Base) & incomparable_constraint(T, Sub))
    # revealed: ty_extensions.ConstraintSet[((T@_ ≠ Base) ∧ (T@_ ≁ Unrelated))]
    reveal_type(not_equivalent_constraint(T, Base) & incomparable_constraint(T, Unrelated))
```

### Intersection of two incomparables

We can only simplify the intersection of two incomparable constraints if they have the same pivot.

```py
from typing import final
from ty_extensions import incomparable_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base)]
    reveal_type(incomparable_constraint(T, Base) & incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[((T@_ ≁ Base) ∧ (T@_ ≁ Super))]
    reveal_type(incomparable_constraint(T, Base) & incomparable_constraint(T, Super))
    # revealed: ty_extensions.ConstraintSet[((T@_ ≁ Base) ∧ (T@_ ≁ Sub))]
    reveal_type(incomparable_constraint(T, Base) & incomparable_constraint(T, Sub))
    # revealed: ty_extensions.ConstraintSet[((T@_ ≁ Base) ∧ (T@_ ≁ Unrelated))]
    reveal_type(incomparable_constraint(T, Base) & incomparable_constraint(T, Unrelated))
```

## Union

The union of two constraint sets requires that the constraints in either set hold. In many cases, we
can simplify the result of an union.

### Different typevars

```py
from ty_extensions import incomparable_constraint, not_equivalent_constraint, range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...
```

We cannot simplify the union of constraints that refer to different typevars.

```py
def _[T, U]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base) ∨ (U@_ ≁ Base)]
    reveal_type(incomparable_constraint(T, Base) | incomparable_constraint(U, Base))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base) ∨ (U@_ ≠ Base)]
    reveal_type(not_equivalent_constraint(T, Base) | not_equivalent_constraint(U, Base))
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Base) ∨ (Sub ≤ U@_ ≤ Base)]
    reveal_type(range_constraint(Sub, T, Base) | range_constraint(Sub, U, Base))
```

Union is reflexive.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base)]
    reveal_type(incomparable_constraint(T, Base) | incomparable_constraint(T, Base))
```

### Union of two ranges

When one of the bounds is entirely contained within the other, the union simplifies to the larger
bounds.

```py
from typing import final
from ty_extensions import range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class SubSub(Sub): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(SubSub ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(SubSub, T, Super) | range_constraint(Sub, T, Base))
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(Sub, T, Super) | range_constraint(Sub, T, Super))
```

Otherwise, the union cannot be simplified.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Base) ∨ (Base ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(Sub, T, Base) | range_constraint(Base, T, Super))
    # revealed: ty_extensions.ConstraintSet[(SubSub ≤ T@_ ≤ Sub) ∨ (Base ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(SubSub, T, Sub) | range_constraint(Base, T, Super))
    # revealed: ty_extensions.ConstraintSet[(SubSub ≤ T@_ ≤ Sub) ∨ (Unrelated ≤ T@_)]
    reveal_type(range_constraint(SubSub, T, Sub) | range_constraint(Unrelated, T, object))
```

In particular, the following does not simplify, even though it seems like it could simplify to
`SubSub ≤ T@_ ≤ Super`. The issue is that there are types that are within the bounds of
`SubSub ≤ T@_ ≤ Super`, but which are not comparable to `Base` or `Sub`, and which therefore should
not be included in the union. An example would be the type that contains all instances of `Super`,
`Base`, and `SubSub` (but _not_ including instances of `Sub`). (We don't have a way to spell that
type at the moment, but it is a valid type.) That type is not in `SubSub ≤ T ≤ Base`, since it
includes `Super`, which is outside the range. It's also not in `Sub ≤ T ≤ Super`, because it does
not include `Sub`. That means it should not be in the union. Since that type _is_ in
`SubSub ≤ T ≤ Super`, it is not correct to simplify the union in this way.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(SubSub ≤ T@_ ≤ Base) ∨ (Sub ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(SubSub, T, Base) | range_constraint(Sub, T, Super))
```

### Union of range and not-equivalent

If the hole of a not-equivalent constraint is within the lower and upper bounds of a range
constraint, the range "fills in" the hole. The resulting union can always be satisfied.

```py
from typing import final
from ty_extensions import not_equivalent_constraint, range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(range_constraint(Sub, T, Super) | not_equivalent_constraint(T, Base))
```

Otherwise the range constraint is subsumed by the not-equivalent constraint.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Super)]
    reveal_type(range_constraint(Sub, T, Base) | not_equivalent_constraint(T, Super))
```

### Union of range and incomparable

When the "pivot" of the incomparable constraint is not comparable with either bound of the range
constraint, the incomparable constraint subsumes the range constraint.

```py
from typing import final
from ty_extensions import incomparable_constraint, range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Unrelated)]
    reveal_type(range_constraint(Base, T, Super) | incomparable_constraint(T, Unrelated))
```

Otherwise, the union cannot be simplified.

```py
from ty_extensions import is_subtype_of

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_ ≤ Super) ∨ (T@_ ≁ Base)]
    reveal_type(range_constraint(Base, T, Super) | incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_ ≤ Super) ∨ (T@_ ≁ Sub)]
    reveal_type(range_constraint(Base, T, Super) | incomparable_constraint(T, Sub))
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_ ≤ Super) ∨ (T@_ ≁ Super)]
    reveal_type(range_constraint(Base, T, Super) | incomparable_constraint(T, Super))
```

### Union of two not-equivalents

Union is reflexive, so the union of a not-equivalent constraint with itself is itself. The union of
two different not-equivalent constraints is always satisfied.

```py
from typing import final
from ty_extensions import not_equivalent_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base)]
    reveal_type(not_equivalent_constraint(T, Base) | not_equivalent_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(not_equivalent_constraint(T, Base) | not_equivalent_constraint(T, Super))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(not_equivalent_constraint(T, Base) | not_equivalent_constraint(T, Sub))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(not_equivalent_constraint(T, Base) | not_equivalent_constraint(T, Unrelated))
```

### Union of not-equivalent and incomparable

When the hole of the non-equivalent constraint and the pivot of the incomparable constraint are not
comparable, then the hole is covered by the incomparable constraint, and the union is therefore
always satisfied.

```py
from typing import final
from ty_extensions import incomparable_constraint, not_equivalent_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(not_equivalent_constraint(T, Unrelated) | incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(not_equivalent_constraint(T, Base) | incomparable_constraint(T, Unrelated))
```

Otherwise, the hole and pivot are comparable, and the non-equivalent constraint subsumes the
incomparable constraint.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base)]
    reveal_type(not_equivalent_constraint(T, Base) | incomparable_constraint(T, Super))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base)]
    reveal_type(not_equivalent_constraint(T, Base) | incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base)]
    reveal_type(not_equivalent_constraint(T, Base) | incomparable_constraint(T, Sub))

    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Super)]
    reveal_type(not_equivalent_constraint(T, Super) | incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base)]
    reveal_type(not_equivalent_constraint(T, Base) | incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Sub)]
    reveal_type(not_equivalent_constraint(T, Sub) | incomparable_constraint(T, Base))
```

### Union of two incomparables

We can only simplify the union of two incomparable constraints if they have the same pivot.

```py
from typing import final
from ty_extensions import incomparable_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base)]
    reveal_type(incomparable_constraint(T, Base) | incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base) ∨ (T@_ ≁ Super)]
    reveal_type(incomparable_constraint(T, Base) | incomparable_constraint(T, Super))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base) ∨ (T@_ ≁ Sub)]
    reveal_type(incomparable_constraint(T, Base) | incomparable_constraint(T, Sub))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base) ∨ (T@_ ≁ Unrelated)]
    reveal_type(incomparable_constraint(T, Base) | incomparable_constraint(T, Unrelated))
```

## Negation

### Negation of a range constraint

In the negation of a range constraint, the typevar must specialize to a type that is not a subtype
of the lower bound, or is not a supertype of the upper bound. Subtyping is a partial order, so one
type is not a subtype of another if it is a _proper_ supertype, or if they are incomparable.

```py
from typing import Never
from ty_extensions import range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[((T@_ ≤ Sub) ∧ (T@_ ≠ Sub)) ∨ (T@_ ≁ Sub) ∨ ((Base ≤ T@_) ∧ (T@_ ≠ Base)) ∨ (T@_ ≁ Base)]
    reveal_type(~range_constraint(Sub, T, Base))
    # revealed: ty_extensions.ConstraintSet[((Base ≤ T@_) ∧ (T@_ ≠ Base)) ∨ (T@_ ≁ Base)]
    reveal_type(~range_constraint(Never, T, Base))
    # revealed: ty_extensions.ConstraintSet[((T@_ ≤ Sub) ∧ (T@_ ≠ Sub)) ∨ (T@_ ≁ Sub)]
    reveal_type(~range_constraint(Sub, T, object))
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(~range_constraint(Never, T, object))
```

### Negation of a not-equivalent constraint

In the negation of a not-equivalent constrant, the typevar must specialize to a type that _is_
equivalent to the hole. The negation does not include types that are incomparable with the hole —
those types are not equivalent to the hole, and are therefore in the original not-equivalent
constraint, not its negation.

```py
from typing import Never
from ty_extensions import not_equivalent_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_ ≤ Base)]
    reveal_type(~not_equivalent_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Sub)]
    reveal_type(~not_equivalent_constraint(T, Sub))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≤ Never)]
    reveal_type(~not_equivalent_constraint(T, Never))
    # revealed: ty_extensions.ConstraintSet[(object ≤ T@_)]
    reveal_type(~not_equivalent_constraint(T, object))
```

### Negation of an incomparable constraint

In the negation of an incomparable constraint, the typevar must specialize to a type that _is_
comparable with (either a subtype _or_ supertype of) the pivot.

```py
from typing import Never
from ty_extensions import incomparable_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≤ Base) ∨ (Base ≤ T@_)]
    reveal_type(~incomparable_constraint(T, Base))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≤ Sub) ∨ (Sub ≤ T@_)]
    reveal_type(~incomparable_constraint(T, Sub))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(~incomparable_constraint(T, Never))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(~incomparable_constraint(T, object))
```
