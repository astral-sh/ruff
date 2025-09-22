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
    # revealed: ty_extensions.ConstraintSet[(T@_ = Base)]
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

### Negated range

A _negated range_ constraint is the opposite of a range constraint: it requires the typevar to _not_
be within a particular lower and upper bound. The typevar can only specialize to a type that is a
strict subtype of the lower bound, a strict supertype of the upper bound, or incomparable to either.

```py
from typing import Any, final, Never, Sequence
from ty_extensions import negated_range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[¬(Sub ≤ T@_ ≤ Super)]
    reveal_type(negated_range_constraint(Sub, T, Super))
```

Every type is a supertype of `Never`, so a lower bound of `Never` is the same as having no lower
bound.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[¬(T@_ ≤ Base)]
    reveal_type(negated_range_constraint(Never, T, Base))
```

Similarly, every type is a subtype of `object`, so an upper bound of `object` is the same as having
no upper bound.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[¬(Base ≤ T@_)]
    reveal_type(negated_range_constraint(Base, T, object))
```

And a negated range constraint with _both_ a lower bound of `Never` and an upper bound of `object`
cannot be satisfied at all.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(negated_range_constraint(Never, T, object))
```

If the lower bound and upper bounds are "inverted" (the upper bound is a subtype of the lower bound)
or incomparable, then the negated range constraint can always be satisfied.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(negated_range_constraint(Super, T, Sub))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(negated_range_constraint(Base, T, Unrelated))
```

The lower and upper bound can be the same type, in which case the typevar can be specialized to any
type other than that specific type.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base)]
    reveal_type(negated_range_constraint(Base, T, Base))
```

Constraints can only refer to fully static types, so the lower and upper bounds are transformed into
their bottom and top materializations, respectively.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[¬(Base ≤ T@_)]
    reveal_type(negated_range_constraint(Base, T, Any))
    # revealed: ty_extensions.ConstraintSet[¬(Sequence[Base] ≤ T@_ ≤ Sequence[object])]
    reveal_type(negated_range_constraint(Sequence[Base], T, Sequence[Any]))

    # revealed: ty_extensions.ConstraintSet[¬(T@_ ≤ Base)]
    reveal_type(negated_range_constraint(Any, T, Base))
    # revealed: ty_extensions.ConstraintSet[¬(Sequence[Never] ≤ T@_ ≤ Sequence[Base])]
    reveal_type(negated_range_constraint(Sequence[Any], T, Sequence[Base]))
```

## Intersection

The intersection of two constraint sets requires that the constraints in both sets hold. In many
cases, we can simplify the result of an intersection.

### Different typevars

```py
from ty_extensions import range_constraint, negated_range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...
```

We cannot simplify the intersection of constraints that refer to different typevars.

```py
def _[T, U]() -> None:
    # revealed: ty_extensions.ConstraintSet[((Sub ≤ T@_ ≤ Base) ∧ (Sub ≤ U@_ ≤ Base))]
    reveal_type(range_constraint(Sub, T, Base) & range_constraint(Sub, U, Base))
    # revealed: ty_extensions.ConstraintSet[(¬(Sub ≤ T@_ ≤ Base) ∧ ¬(Sub ≤ U@_ ≤ Base))]
    reveal_type(negated_range_constraint(Sub, T, Base) & negated_range_constraint(Sub, U, Base))
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
    # revealed: ty_extensions.ConstraintSet[(T@_ = Base)]
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

### Intersection of a range and a negated range

The bounds of the range constraint provide a range of types that should be included; the bounds of
the negated range constraint provide a "hole" of types that should not be included. We can think of
the intersection as removing the hole from the range constraint.

```py
from typing import final, Never
from ty_extensions import range_constraint, negated_range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class SubSub(Sub): ...

@final
class Unrelated: ...
```

If the negative range completely contains the positive range, then the intersection is empty.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(Sub, T, Base) & negated_range_constraint(SubSub, T, Super))
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(Sub, T, Base) & negated_range_constraint(Sub, T, Base))
```

If the negative range is disjoint from the positive range, the negative range doesn't remove
anything; the intersection is the positive range.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Base)]
    reveal_type(range_constraint(Sub, T, Base) & negated_range_constraint(Never, T, Unrelated))
    # revealed: ty_extensions.ConstraintSet[(SubSub ≤ T@_ ≤ Sub)]
    reveal_type(range_constraint(SubSub, T, Sub) & negated_range_constraint(Base, T, Super))
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(Base, T, Super) & negated_range_constraint(SubSub, T, Sub))
```

Otherwise we clip the negative constraint to the mininum range that overlaps with the positive
range.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[((SubSub ≤ T@_ ≤ Base) ∧ ¬(Sub ≤ T@_ ≤ Base))]
    reveal_type(range_constraint(SubSub, T, Base) & negated_range_constraint(Sub, T, Super))
    # revealed: ty_extensions.ConstraintSet[((SubSub ≤ T@_ ≤ Super) ∧ ¬(Sub ≤ T@_ ≤ Base))]
    reveal_type(range_constraint(SubSub, T, Super) & negated_range_constraint(Sub, T, Base))
```

### Intersection of two negated ranges

When one of the bounds is entirely contained within the other, the intersection simplifies to the
smaller constraint. For negated ranges, the smaller constraint is the one with the larger "hole".

```py
from typing import final
from ty_extensions import negated_range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class SubSub(Sub): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[¬(SubSub ≤ T@_ ≤ Super)]
    reveal_type(negated_range_constraint(SubSub, T, Super) & negated_range_constraint(Sub, T, Base))
    # revealed: ty_extensions.ConstraintSet[¬(Sub ≤ T@_ ≤ Super)]
    reveal_type(negated_range_constraint(Sub, T, Super) & negated_range_constraint(Sub, T, Super))
```

Otherwise, the union cannot be simplified.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(¬(Sub ≤ T@_ ≤ Base) ∧ ¬(Base ≤ T@_ ≤ Super))]
    reveal_type(negated_range_constraint(Sub, T, Base) & negated_range_constraint(Base, T, Super))
    # revealed: ty_extensions.ConstraintSet[(¬(SubSub ≤ T@_ ≤ Sub) ∧ ¬(Base ≤ T@_ ≤ Super))]
    reveal_type(negated_range_constraint(SubSub, T, Sub) & negated_range_constraint(Base, T, Super))
    # revealed: ty_extensions.ConstraintSet[(¬(SubSub ≤ T@_ ≤ Sub) ∧ ¬(Unrelated ≤ T@_))]
    reveal_type(negated_range_constraint(SubSub, T, Sub) & negated_range_constraint(Unrelated, T, object))
```

In particular, the following does not simplify, even though it seems like it could simplify to
`¬(SubSub ≤ T@_ ≤ Super)`. The issue is that there are types that are within the bounds of
`SubSub ≤ T@_ ≤ Super`, but which are not comparable to `Base` or `Sub`, and which therefore should
be included in the union. An example would be the type that contains all instances of `Super`,
`Base`, and `SubSub` (but _not_ including instances of `Sub`). (We don't have a way to spell that
type at the moment, but it is a valid type.) That type is not in `SubSub ≤ T ≤ Base`, since it
includes `Super`, which is outside the range. It's also not in `Sub ≤ T ≤ Super`, because it does
not include `Sub`. That means it should be in the union. (Remember that for negated range
constraints, the lower and upper bounds define the "hole" of types that are _not_ allowed.) Since
that type _is_ in `SubSub ≤ T ≤ Super`, it is not correct to simplify the union in this way.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[(¬(SubSub ≤ T@_ ≤ Base) ∧ ¬(Sub ≤ T@_ ≤ Super))]
    reveal_type(negated_range_constraint(SubSub, T, Base) & negated_range_constraint(Sub, T, Super))
```

## Union

The union of two constraint sets requires that the constraints in either set hold. In many cases, we
can simplify the result of an union.

### Different typevars

```py
from ty_extensions import range_constraint, negated_range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...
```

We cannot simplify the union of constraints that refer to different typevars.

```py
def _[T, U]() -> None:
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Base) ∨ (Sub ≤ U@_ ≤ Base)]
    reveal_type(range_constraint(Sub, T, Base) | range_constraint(Sub, U, Base))
    # revealed: ty_extensions.ConstraintSet[¬(Sub ≤ T@_ ≤ Base) ∨ ¬(Sub ≤ U@_ ≤ Base)]
    reveal_type(negated_range_constraint(Sub, T, Base) | negated_range_constraint(Sub, U, Base))
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

### Union of a range and a negated range

The bounds of the range constraint provide a range of types that should be included; the bounds of
the negated range constraint provide a "hole" of types that should not be included. We can think of
the union as filling part of the hole with the types from the range constraint.

```py
from typing import final, Never
from ty_extensions import range_constraint, negated_range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class SubSub(Sub): ...

@final
class Unrelated: ...
```

If the positive range completely contains the negative range, then the union is always satisfied.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(negated_range_constraint(Sub, T, Base) | range_constraint(SubSub, T, Super))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(negated_range_constraint(Sub, T, Base) | range_constraint(Sub, T, Base))
```

If the negative range is disjoint from the positive range, the positive range doesn't add anything;
the union is the negative range.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[¬(Sub ≤ T@_ ≤ Base)]
    reveal_type(negated_range_constraint(Sub, T, Base) | range_constraint(Never, T, Unrelated))
    # revealed: ty_extensions.ConstraintSet[¬(SubSub ≤ T@_ ≤ Sub)]
    reveal_type(negated_range_constraint(SubSub, T, Sub) | range_constraint(Base, T, Super))
    # revealed: ty_extensions.ConstraintSet[¬(Base ≤ T@_ ≤ Super)]
    reveal_type(negated_range_constraint(Base, T, Super) | range_constraint(SubSub, T, Sub))
```

Otherwise we clip the positive constraint to the mininum range that overlaps with the negative
range.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[¬(SubSub ≤ T@_ ≤ Base) ∨ (Sub ≤ T@_ ≤ Base)]
    reveal_type(negated_range_constraint(SubSub, T, Base) | range_constraint(Sub, T, Super))
    # revealed: ty_extensions.ConstraintSet[¬(SubSub ≤ T@_ ≤ Super) ∨ (Sub ≤ T@_ ≤ Base)]
    reveal_type(negated_range_constraint(SubSub, T, Super) | range_constraint(Sub, T, Base))
```

### Union of two negated ranges

The union of two negated ranges has a hole where the ranges "overlap".

```py
from typing import final
from ty_extensions import negated_range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class SubSub(Sub): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[¬(Sub ≤ T@_ ≤ Base)]
    reveal_type(negated_range_constraint(SubSub, T, Base) | negated_range_constraint(Sub, T, Super))
    # revealed: ty_extensions.ConstraintSet[¬(Sub ≤ T@_ ≤ Base)]
    reveal_type(negated_range_constraint(SubSub, T, Super) | negated_range_constraint(Sub, T, Base))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base)]
    reveal_type(negated_range_constraint(Sub, T, Base) | negated_range_constraint(Base, T, Super))
    # revealed: ty_extensions.ConstraintSet[¬(Sub ≤ T@_ ≤ Super)]
    reveal_type(negated_range_constraint(Sub, T, Super) | negated_range_constraint(Sub, T, Super))
```

If the holes don't overlap, the union is always satisfied.

```py
def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(negated_range_constraint(SubSub, T, Sub) | negated_range_constraint(Base, T, Super))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(negated_range_constraint(SubSub, T, Sub) | negated_range_constraint(Unrelated, T, object))
```

## Negation

### Negation of a range constraint

```py
from typing import Never
from ty_extensions import range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

def _[T]() -> None:
    # revealed: ty_extensions.ConstraintSet[¬(Sub ≤ T@_ ≤ Base)]
    reveal_type(~range_constraint(Sub, T, Base))
    # revealed: ty_extensions.ConstraintSet[¬(T@_ ≤ Base)]
    reveal_type(~range_constraint(Never, T, Base))
    # revealed: ty_extensions.ConstraintSet[¬(Sub ≤ T@_)]
    reveal_type(~range_constraint(Sub, T, object))
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(~range_constraint(Never, T, object))
```

The union of a range constraint and its negation should always be satisfiable.

```py
def _[T]() -> None:
    constraint = range_constraint(Sub, T, Base)
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(constraint | ~constraint)
```
