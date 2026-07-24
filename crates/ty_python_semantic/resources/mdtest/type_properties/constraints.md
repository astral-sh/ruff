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
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # (Sub ≤ T@_ ≤ Super)
    ConstraintSet.range(Sub, T, Super)
```

Every type is a supertype of `Never`, so a lower bound of `Never` is the same as having no lower
bound.

```py
def _[T]() -> None:
    # (T@_ ≤ Base)
    ConstraintSet.range(Never, T, Base)
```

Similarly, every type is a subtype of `object`, so an upper bound of `object` is the same as having
no upper bound.

```py
def _[T]() -> None:
    # (Base ≤ T@_)
    ConstraintSet.range(Base, T, object)
```

And a range constraint with a lower bound of `Never` and an upper bound of `object` allows the
typevar to take on any type. We treat this differently than the `always` constraint set. During
specialization inference, that allows us to distinguish between not constraining a typevar (and
therefore falling back on its default specialization) and explicitly constraining it to any subtype
of `object`.

```py
def _[T]() -> None:
    # (T@_ = *)
    ConstraintSet.range(Never, T, object)
```

If the lower bound and upper bounds are "inverted" (the upper bound is a subtype of the lower bound)
or incomparable, then there is no type that can satisfy the constraint.

```py
def _[T]() -> None:
    static_assert(not ConstraintSet.range(Super, T, Sub))
    static_assert(not ConstraintSet.range(Base, T, Unrelated))
```

The lower and upper bound can be the same type, in which case the typevar can only be specialized to
that specific type.

```py
def _[T]() -> None:
    # (T@_ = Base)
    ConstraintSet.range(Base, T, Base)
```

Gradual lower and upper bounds are not eagerly simplified to their bottom and top materializations.

```py
def _[T]() -> None:
    constraints = ConstraintSet.range(Base, T, Any)
    expected = ConstraintSet.range(Base, T, object)
    static_assert(constraints != expected)

    constraints = ConstraintSet.range(Sequence[Base], T, Sequence[Any])
    expected = ConstraintSet.range(Sequence[Base], T, Sequence[object])
    static_assert(constraints != expected)

    constraints = ConstraintSet.range(Any, T, Base)
    expected = ConstraintSet.range(Never, T, Base)
    static_assert(constraints != expected)

    constraints = ConstraintSet.range(Sequence[Any], T, Sequence[Base])
    expected = ConstraintSet.range(Sequence[Never], T, Sequence[Base])
    static_assert(constraints != expected)
```

### Negated range

A _negated range_ constraint is the opposite of a range constraint: it requires the typevar to _not_
be within a particular lower and upper bound. The typevar can only specialize to a type that is a
strict subtype of the lower bound, a strict supertype of the upper bound, or incomparable to either.

```pyi
from typing import Any, final, Never, Sequence
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def _[T]() -> None:
    # ¬(Sub ≤ T@_ ≤ Super)
    ~ConstraintSet.range(Sub, T, Super)
```

Every type is a supertype of `Never`, so a lower bound of `Never` is the same as having no lower
bound.

```pyi
def _[T]() -> None:
    # ¬(T@_ ≤ Base)
    ~ConstraintSet.range(Never, T, Base)
```

Similarly, every type is a subtype of `object`, so an upper bound of `object` is the same as having
no upper bound.

```pyi
def _[T]() -> None:
    # ¬(Base ≤ T@_)
    ~ConstraintSet.range(Base, T, object)
```

And a negated range constraint with _both_ a lower bound of `Never` and an upper bound of `object`
cannot be satisfied at all.

```pyi
def _[T]() -> None:
    # (T@_ ≠ *)
    ~ConstraintSet.range(Never, T, object)
```

If the lower bound and upper bounds are "inverted" (the upper bound is a subtype of the lower bound)
or incomparable, then the negated range constraint can always be satisfied.

```pyi
def _[T]() -> None:
    static_assert(~ConstraintSet.range(Super, T, Sub))
    static_assert(~ConstraintSet.range(Base, T, Unrelated))
```

The lower and upper bound can be the same type, in which case the typevar can be specialized to any
type other than that specific type.

```pyi
def _[T]() -> None:
    # (T@_ ≠ Base)
    ~ConstraintSet.range(Base, T, Base)
```

Gradual lower and upper bounds are not eagerly simplified to their bottom and top materializations.

```pyi
def _[T]() -> None:
    constraints = ~ConstraintSet.range(Base, T, Any)
    expected = ~ConstraintSet.range(Base, T, object)
    static_assert(constraints != expected)

    constraints = ~ConstraintSet.range(Sequence[Base], T, Sequence[Any])
    expected = ~ConstraintSet.range(Sequence[Base], T, Sequence[object])
    static_assert(constraints != expected)

    constraints = ~ConstraintSet.range(Any, T, Base)
    expected = ~ConstraintSet.range(Never, T, Base)
    static_assert(constraints != expected)

    constraints = ~ConstraintSet.range(Sequence[Any], T, Sequence[Base])
    expected = ~ConstraintSet.range(Sequence[Never], T, Sequence[Base])
    static_assert(constraints != expected)
```

A negated _type_ is not the same thing as a negated _range_.

```pyi
def _[T]() -> None:
    negated_type = ConstraintSet.range(Never, T, ~int)
    negated_constraint = ~ConstraintSet.range(Never, T, int)
    static_assert(negated_type != negated_constraint)
```

## Intersection

The intersection of two constraint sets requires that the constraints in both sets hold. In many
cases, we can simplify the result of an intersection.

### Different typevars

```py
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...
```

We cannot simplify the intersection of constraints that refer to different typevars.

```py
def _[T, U]() -> None:
    # (Sub ≤ T@_ ≤ Base) ∧ (Sub ≤ U@_ ≤ Base)
    ConstraintSet.range(Sub, T, Base) & ConstraintSet.range(Sub, U, Base)
    # ¬(Sub ≤ T@_ ≤ Base) ∧ ¬(Sub ≤ U@_ ≤ Base)
    ~ConstraintSet.range(Sub, T, Base) & ~ConstraintSet.range(Sub, U, Base)
```

### Intersection of two ranges

The intersection of two ranges is where the ranges "overlap".

```pyi
from typing import final
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class SubSub(Sub): ...

@final
class Unrelated: ...

def _[T]() -> None:
    constraints = ConstraintSet.range(SubSub, T, Base) & ConstraintSet.range(Sub, T, Super)
    expected = ConstraintSet.range(Sub, T, Base)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(SubSub, T, Super) & ConstraintSet.range(Sub, T, Base)
    expected = ConstraintSet.range(Sub, T, Base)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Sub, T, Base) & ConstraintSet.range(Base, T, Super)
    expected = ConstraintSet.range(Base, T, Base)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Sub, T, Super) & ConstraintSet.range(Sub, T, Super)
    expected = ConstraintSet.range(Sub, T, Super)
    static_assert(constraints == expected)
```

If they don't overlap, the intersection is empty.

```pyi
def _[T]() -> None:
    static_assert(not ConstraintSet.range(SubSub, T, Sub) & ConstraintSet.range(Base, T, Super))
    static_assert(not ConstraintSet.range(SubSub, T, Sub) & ConstraintSet.range(Unrelated, T, object))
```

Expanding on this, when intersecting two upper bounds constraints (`(T ≤ Base) ∧ (T ≤ Other)`), we
intersect the upper bounds. Any type that satisfies both `T ≤ Base` and `T ≤ Other` must necessarily
satisfy their intersection `T ≤ Base & Other`, and vice versa.

```pyi
from typing import Never

# This is not final, so it's possible for a subclass to inherit from both Base and Other.
class Other: ...

def upper_bounds[T]():
    # (T@upper_bounds ≤ Base & Other)
    intersection_type = ConstraintSet.range(Never, T, Base & Other)
    # (T@upper_bounds ≤ Base) ∧ (T@upper_bounds ≤ Other)
    intersection_constraint = ConstraintSet.range(Never, T, Base) & ConstraintSet.range(Never, T, Other)
    static_assert(intersection_type == intersection_constraint)
```

For an intersection of two lower bounds constraints (`(Base ≤ T) ∧ (Other ≤ T)`), we union the lower
bounds. Any type that satisfies both `Base ≤ T` and `Other ≤ T` must necessarily satisfy their union
`Base | Other ≤ T`, and vice versa.

```pyi
def lower_bounds[T]():
    # (Base | Other ≤ T@lower_bounds)
    union_type = ConstraintSet.range(Base | Other, T, object)
    # (Base ≤ T@upper_bounds) ∧ (Other ≤ T@upper_bounds)
    intersection_constraint = ConstraintSet.range(Base, T, object) & ConstraintSet.range(Other, T, object)
    static_assert(union_type == intersection_constraint)
```

### Intersection of a range and a negated range

The bounds of the range constraint provide a range of types that should be included; the bounds of
the negated range constraint provide a "hole" of types that should not be included. We can think of
the intersection as removing the hole from the range constraint.

```py
from typing import final, Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

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
    static_assert(not ConstraintSet.range(Sub, T, Base) & ~ConstraintSet.range(SubSub, T, Super))
    static_assert(not ConstraintSet.range(Sub, T, Base) & ~ConstraintSet.range(Sub, T, Base))
```

If the negative range is disjoint from the positive range, the negative range doesn't remove
anything; the intersection is the positive range.

```py
def _[T]() -> None:
    constraints = ConstraintSet.range(Sub, T, Base) & ~ConstraintSet.range(Never, T, Unrelated)
    expected = ConstraintSet.range(Sub, T, Base)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(SubSub, T, Sub) & ~ConstraintSet.range(Base, T, Super)
    expected = ConstraintSet.range(SubSub, T, Sub)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Base, T, Super) & ~ConstraintSet.range(SubSub, T, Sub)
    expected = ConstraintSet.range(Base, T, Super)
    static_assert(constraints == expected)
```

Otherwise we clip the negative constraint to the minimum range that overlaps with the positive
range.

```py
def _[T]() -> None:
    constraints = ConstraintSet.range(SubSub, T, Base) & ~ConstraintSet.range(Sub, T, Super)
    expected = ConstraintSet.range(SubSub, T, Base) & ~ConstraintSet.range(Sub, T, Base)
    static_assert(constraints == expected)
```

### Intersection of two negated ranges

When one of the bounds is entirely contained within the other, the intersection simplifies to the
smaller constraint. For negated ranges, the smaller constraint is the one with the larger "hole".

```py
from typing import final
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class SubSub(Sub): ...

@final
class Unrelated: ...

def _[T]() -> None:
    constraints = ~ConstraintSet.range(SubSub, T, Super) & ~ConstraintSet.range(Sub, T, Base)
    expected = ~ConstraintSet.range(SubSub, T, Super)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Sub, T, Super) & ~ConstraintSet.range(Sub, T, Super)
    expected = ~ConstraintSet.range(Sub, T, Super)
    static_assert(constraints == expected)
```

Otherwise, the intersection cannot be simplified.

```py
def _[T]() -> None:
    # ¬(Base ≤ T@_ ≤ Super) ∧ ¬(Sub ≤ T@_ ≤ Base))
    ~ConstraintSet.range(Sub, T, Base) & ~ConstraintSet.range(Base, T, Super)
    # ¬(Base ≤ T@_ ≤ Super) ∧ ¬(SubSub ≤ T@_ ≤ Sub))
    ~ConstraintSet.range(SubSub, T, Sub) & ~ConstraintSet.range(Base, T, Super)
    # ¬(SubSub ≤ T@_ ≤ Sub) ∧ ¬(Unrelated ≤ T@_)
    ~ConstraintSet.range(SubSub, T, Sub) & ~ConstraintSet.range(Unrelated, T, object)
```

In particular, the following does not simplify, even though it seems like it could simplify to
`¬(SubSub ≤ T@_ ≤ Super)`. The issue is that there are types that are within the bounds of
`SubSub ≤ T@_ ≤ Super`, but which are not comparable to `Base` or `Sub`, and which therefore should
be included in the intersection. An example would be the type that contains all instances of
`Super`, `Base`, and `SubSub` (but _not_ including instances of `Sub`). (We don't have a way to
spell that type at the moment, but it is a valid type.) That type is not in `SubSub ≤ T ≤ Base`,
since it includes `Super`, which is outside the range. It's also not in `Sub ≤ T ≤ Super`, because
it does not include `Sub`. That means it should be in the intersection. (Remember that for negated
range constraints, the lower and upper bounds define the "hole" of types that are _not_ allowed.)
Since that type _is_ in `SubSub ≤ T ≤ Super`, it is not correct to simplify the intersection in this
way.

```py
def _[T]() -> None:
    # (¬(Sub ≤ T@_ ≤ Super) ∧ ¬(SubSub ≤ T@_ ≤ Base))
    ~ConstraintSet.range(SubSub, T, Base) & ~ConstraintSet.range(Sub, T, Super)
```

## Union

The union of two constraint sets requires that the constraints in either set hold. In many cases, we
can simplify the result of an union.

### Different typevars

```py
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...
```

We cannot simplify the union of constraints that refer to different typevars.

```py
def _[T, U]() -> None:
    # (Sub ≤ T@_ ≤ Base) ∨ (Sub ≤ U@_ ≤ Base)
    ConstraintSet.range(Sub, T, Base) | ConstraintSet.range(Sub, U, Base)
    # ¬(Sub ≤ T@_ ≤ Base) ∨ ¬(Sub ≤ U@_ ≤ Base)
    ~ConstraintSet.range(Sub, T, Base) | ~ConstraintSet.range(Sub, U, Base)
```

### Union of two ranges

When one of the bounds is entirely contained within the other, the union simplifies to the larger
bounds.

```py
from typing import final
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class SubSub(Sub): ...

@final
class Unrelated: ...

def _[T]() -> None:
    constraints = ConstraintSet.range(SubSub, T, Super) | ConstraintSet.range(Sub, T, Base)
    expected = ConstraintSet.range(SubSub, T, Super)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Sub, T, Super) | ConstraintSet.range(Sub, T, Super)
    expected = ConstraintSet.range(Sub, T, Super)
    static_assert(constraints == expected)
```

Otherwise, the union cannot be simplified.

```py
def _[T]() -> None:
    # (Base ≤ T@_ ≤ Super) ∨ (Sub ≤ T@_ ≤ Base)
    ConstraintSet.range(Sub, T, Base) | ConstraintSet.range(Base, T, Super)
    # (Base ≤ T@_ ≤ Super) ∨ (SubSub ≤ T@_ ≤ Sub)
    ConstraintSet.range(SubSub, T, Sub) | ConstraintSet.range(Base, T, Super)
    # (SubSub ≤ T@_ ≤ Sub) ∨ (Unrelated ≤ T@_)
    ConstraintSet.range(SubSub, T, Sub) | ConstraintSet.range(Unrelated, T, object)
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
    # (Sub ≤ T@_ ≤ Super) ∨ (SubSub ≤ T@_ ≤ Base)
    ConstraintSet.range(SubSub, T, Base) | ConstraintSet.range(Sub, T, Super)
```

The union of two upper bound constraints (`(T ≤ Base) ∨ (T ≤ Other)`) is different than the single
range constraint involving the corresponding union type (`T ≤ Base | Other`). There are types (such
as `T = Base | Other`) that satisfy the union type, but not the union constraint. But every type
that satisfies the union constraint satisfies the union type.

```py
from typing import Never

# This is not final, so it's possible for a subclass to inherit from both Base and Other.
class Other: ...

def union[T]():
    # (T@union ≤ Base | Other)
    union_type = ConstraintSet.range(Never, T, Base | Other)
    # (T@union ≤ Base) ∨ (T@union ≤ Other)
    union_constraint = ConstraintSet.range(Never, T, Base) | ConstraintSet.range(Never, T, Other)

    # (T = Base | Other) satisfies (T ≤ Base | Other) but not (T ≤ Base ∨ T ≤ Other)
    specialization = ConstraintSet.range(Base | Other, T, Base | Other)
    static_assert(specialization.satisfies(union_type))
    static_assert(not specialization.satisfies(union_constraint))

    # Every specialization that satisfies (T ≤ Base ∨ T ≤ Other) also satisfies
    # (T ≤ Base | Other)
    static_assert(union_constraint.satisfies(union_type))
```

These relationships are reversed for unions involving lower bounds. `T = Base` is an example that
satisfies the union constraint (`(Base ≤ T) ∨ (Other ≤ T)`) but not the union type
(`Base | Other ≤ T`). And every type that satisfies the union type satisfies the union constraint.

```py
def union[T]():
    # (Base | Other ≤ T@union)
    union_type = ConstraintSet.range(Base | Other, T, object)
    # (Base ≤ T@union) ∨ (Other ≤ T@union)
    union_constraint = ConstraintSet.range(Base, T, object) | ConstraintSet.range(Other, T, object)

    # (T = Base) satisfies (Base ≤ T ∨ Other ≤ T) but not (Base | Other ≤ T)
    specialization = ConstraintSet.range(Base, T, Base)
    static_assert(not specialization.satisfies(union_type))
    static_assert(specialization.satisfies(union_constraint))

    # Every specialization that satisfies (Base | Other ≤ T) also satisfies
    # (Base ≤ T ∨ Other ≤ T)
    static_assert(union_type.satisfies(union_constraint))
```

### Union of a range and a negated range

The bounds of the range constraint provide a range of types that should be included; the bounds of
the negated range constraint provide a "hole" of types that should not be included. We can think of
the union as filling part of the hole with the types from the range constraint.

```py
from typing import final, Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

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
    static_assert(~ConstraintSet.range(Sub, T, Base) | ConstraintSet.range(SubSub, T, Super))
    static_assert(~ConstraintSet.range(Sub, T, Base) | ConstraintSet.range(Sub, T, Base))
```

If the negative range is disjoint from the positive range, the positive range doesn't add anything;
the union is the negative range.

```py
def _[T]() -> None:
    constraints = ~ConstraintSet.range(Sub, T, Base) | ConstraintSet.range(Never, T, Unrelated)
    expected = ~ConstraintSet.range(Sub, T, Base)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(SubSub, T, Sub) | ConstraintSet.range(Base, T, Super)
    expected = ~ConstraintSet.range(SubSub, T, Sub)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Base, T, Super) | ConstraintSet.range(SubSub, T, Sub)
    expected = ~ConstraintSet.range(Base, T, Super)
    static_assert(constraints == expected)
```

Otherwise we clip the positive constraint to the minimum range that overlaps with the negative
range.

```py
def _[T]() -> None:
    constraints = ~ConstraintSet.range(SubSub, T, Base) | ConstraintSet.range(Sub, T, Super)
    expected = ~ConstraintSet.range(SubSub, T, Base) | ConstraintSet.range(Sub, T, Base)
    static_assert(constraints == expected)
```

### Union of two negated ranges

The union of two negated ranges has a hole where the ranges "overlap".

```py
from typing import final
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...
class SubSub(Sub): ...

@final
class Unrelated: ...

def _[T]() -> None:
    constraints = ~ConstraintSet.range(SubSub, T, Base) | ~ConstraintSet.range(Sub, T, Super)
    expected = ~ConstraintSet.range(Sub, T, Base)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(SubSub, T, Super) | ~ConstraintSet.range(Sub, T, Base)
    expected = ~ConstraintSet.range(Sub, T, Base)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Sub, T, Base) | ~ConstraintSet.range(Base, T, Super)
    expected = ~ConstraintSet.range(Base, T, Base)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Sub, T, Super) | ~ConstraintSet.range(Sub, T, Super)
    expected = ~ConstraintSet.range(Sub, T, Super)
    static_assert(constraints == expected)
```

If the holes don't overlap, the union is always satisfied.

```py
def _[T]() -> None:
    static_assert(~ConstraintSet.range(SubSub, T, Sub) | ~ConstraintSet.range(Base, T, Super))
    static_assert(~ConstraintSet.range(SubSub, T, Sub) | ~ConstraintSet.range(Unrelated, T, object))
```

## Negation

### Negation of a range constraint

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...

def _[T]() -> None:
    # ¬(Sub ≤ T@_ ≤ Base)
    ~ConstraintSet.range(Sub, T, Base)
    # ¬(T@_ ≤ Base)
    ~ConstraintSet.range(Never, T, Base)
    # ¬(Sub ≤ T@_)
    ~ConstraintSet.range(Sub, T, object)
    # (T@_ ≠ *)
    ~ConstraintSet.range(Never, T, object)
```

The union of a range constraint and its negation should always be satisfiable.

```py
def _[T]() -> None:
    constraint = ConstraintSet.range(Sub, T, Base)
    static_assert(constraint | ~constraint)
```

### Negation of constraints involving two variables

```py
from typing import final, Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Base: ...

@final
class Unrelated: ...

def _[T, U]() -> None:
    # ¬(T@_ ≤ Base) ∨ ¬(U@_ ≤ Base)
    ~(ConstraintSet.range(Never, T, Base) & ConstraintSet.range(Never, U, Base))
```

The union of a constraint and its negation should always be satisfiable.

```py
def _[T, U]() -> None:
    c1 = ConstraintSet.range(Never, T, Base) & ConstraintSet.range(Never, U, Base)
    static_assert(c1 | ~c1)
    static_assert(~c1 | c1)

    c2 = ConstraintSet.range(Unrelated, T, object) & ConstraintSet.range(Unrelated, U, object)
    static_assert(c2 | ~c2)
    static_assert(~c2 | c2)

    union = c1 | c2
    static_assert(union | ~union)
    static_assert(~union | union)
```

## Typevar ordering

Constraints can relate two typevars — i.e., `S ≤ T`. We could encode that in one of two ways:
`Never ≤ S ≤ T` or `S ≤ T ≤ object`. In other words, we can decide whether `S` or `T` is the typevar
being constrained. The other is then the lower or upper bound of the constraint. To handle this, we
enforce an arbitrary ordering on typevars, and always place the constraint on the "earlier" typevar.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def f[S, T]():
    # (S@f ≤ T@f)
    c1 = ConstraintSet.range(Never, S, T)
    c2 = ConstraintSet.range(S, T, object)
    static_assert(c1 == c2)

def f[T, S]():
    # (S@f ≤ T@f)
    c1 = ConstraintSet.range(Never, S, T)
    c2 = ConstraintSet.range(S, T, object)
    static_assert(c1 == c2)
```

Equivalence constraints are similar; internally we arbitrarily choose the "earlier" typevar to be
the constraint, and the other the bound.

```py
def f[S, T]():
    # (S@f = T@f)
    c1 = ConstraintSet.range(T, S, T)
    c2 = ConstraintSet.range(S, T, S)
    static_assert(c1 == c2)

def f[T, S]():
    # (S@f = T@f)
    c1 = ConstraintSet.range(T, S, T)
    c2 = ConstraintSet.range(S, T, S)
    static_assert(c1 == c2)
```

But in the case of `S ≤ T ≤ U`, we end up with an ambiguity. Depending on the typevar ordering, that
might represented internally as `S ≤ T ≤ U`, or as `(S ≤ T) ∧ (T ≤ U)`. However, this should not
affect any uses of the constraint set.

```py
def f[S, T, U]():
    # Could be either of:
    #   (S@f ≤ T@f ≤ U@f)
    #   (S@f ≤ T@f) ∧ (T@f ≤ U@f)
    ConstraintSet.range(S, T, U)
    ...
```

## Other simplifications

### Ordering of intersection and union elements

The ordering of elements in a union or intersection do not affect what types satisfy a constraint
set.

```pyi
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def f[T]():
    c1 = ConstraintSet.range(Never, T, str | int)
    c2 = ConstraintSet.range(Never, T, int | str)
    static_assert(c1 == c2)

    c1 = ConstraintSet.range(Never, T, str & int)
    c2 = ConstraintSet.range(Never, T, int & str)
    static_assert(c1 == c2)
```

### Constraints on the same typevar

Any particular specialization maps each typevar to one type. That means it's not useful to constrain
a typevar with itself as an upper or lower bound. No matter what type the typevar is specialized to,
that type is always a subtype of itself. (Remember that typevars are only specialized to fully
static types.)

```pyi
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def same_typevar[T]():
    constraints = ConstraintSet.range(Never, T, T)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(T, T, object)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(T, T, T)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)
```

This is also true when the typevar appears in a union in the upper bound, or in an intersection in
the lower bound. (Note that this lines up with how we simplify the intersection of two constraints,
as shown above.)

```pyi
def same_typevar[T]():
    constraints = ConstraintSet.range(Never, T, T | None)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(T & None, T, object)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(T & None, T, T | None)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)
```

Similarly, if the lower bound is an intersection containing the _negation_ of the typevar, then the
constraint set can never be satisfied, since every type is disjoint with its negation.

```pyi
def same_typevar[T]():
    constraints = ConstraintSet.range(~T & None, T, object)
    expected = ~ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(~T, T, object)
    expected = ~ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)
```

## Existential quantification

Existential quantification removes the listed typevars from a constraint set. Any constraints that
do not involve those typevars must remain in the result. The result holds whenever _at least one_
valid assignment to the quantified variables satisfies the expression being quantified over.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def preserves_remaining_conjunct[T, U]() -> None:
    t_int = ConstraintSet.range(int, T, int)
    u_str = ConstraintSet.range(str, U, str)
    quantified = (t_int & u_str).exists(tuple[U])
    static_assert(quantified == t_int)

def satisfies_uncertain_disjunct[T, U]() -> None:
    t_int = ConstraintSet.range(int, T, int)
    u_str = ConstraintSet.range(str, U, str)
    quantified = (t_int | u_str).exists(tuple[U])
    static_assert(quantified == ConstraintSet.always())

def no_typevars_is_identity[T]() -> None:
    constraints = ConstraintSet.range(Never, T, int)
    static_assert(constraints.exists(tuple[()]) == constraints)
```

## Universal quantification

Universal quantification removes the listed typevars from a constraint set. Any constraints that do
not involve those typevars must remain in the result. The result holds whenever _every_ valid
assignment to the quantified variables satisfies the expression being quantified over.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def preserves_uncertain_disjunct[T, U]() -> None:
    t_int = ConstraintSet.range(int, T, int)
    u_str = ConstraintSet.range(str, U, str)
    quantified = (t_int | u_str).for_all(tuple[U])
    static_assert(quantified == t_int)

def removes_multiple_typevars[T, U]() -> None:
    t_int = ConstraintSet.range(int, T, int)
    u_str = ConstraintSet.range(str, U, str)
    quantified = (t_int | u_str).for_all(tuple[T, U])
    static_assert(quantified == ConstraintSet.never())

def no_typevars_is_identity[T]() -> None:
    constraints = ConstraintSet.range(Never, T, int)
    static_assert(constraints.for_all(tuple[()]) == constraints)
```

The order of existential and universal quantifiers matters. For each target truth assignment there
is some matching source truth assignment, but no single source truth assignment matches every target
truth assignment.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def quantifier_order[S, T]() -> None:
    source_is_int = ConstraintSet.range(int, S, int)
    target_is_int = ConstraintSet.range(int, T, int)
    equal = source_is_int.satisfies(target_is_int) & target_is_int.satisfies(source_is_int)

    # ∀T.∃S.equal(S, T)
    forall_target_exists_source = equal.exists(tuple[S]).for_all(tuple[T])
    static_assert(forall_target_exists_source == ConstraintSet.always())

    # ∃S.∀T.equal(S, T)
    exists_source_forall_target = equal.for_all(tuple[T]).exists(tuple[S])
    static_assert(exists_source_forall_target == ConstraintSet.never())
```

## Gradual constraints

Constraint-set assignability preserves gradual types. While constraints on the materializations
gradual types themselves are flattened to sentinel value, they may contribute to constraints to
other inferable type variables.

```py
from typing import Any
from ty_extensions import static_assert
from ty_extensions._internal import (
    ConstraintSet,
    is_assignable_to,
    is_constraint_set_assignable_to,
)

gradual = is_constraint_set_assignable_to(Any, int)

# revealed: ConstraintSet[bool]
reveal_type(gradual)

# revealed: ConstraintSet[gradual]
reveal_type(gradual.with_detailed_display())

static_assert(gradual == gradual)
static_assert(gradual != ConstraintSet.always())
static_assert(gradual != ConstraintSet.never())
static_assert(gradual.satisfies(gradual))
static_assert((gradual | ConstraintSet.never()) == gradual)
static_assert((ConstraintSet.never() | gradual) == gradual)
static_assert((gradual & ConstraintSet.always()) == gradual)
static_assert((ConstraintSet.always() & gradual) == gradual)
static_assert(~gradual == gradual)
static_assert(gradual)
static_assert(is_assignable_to(Any, int))

def _[T]() -> None:
    informative = ConstraintSet.range(int, T, object)
    static_assert((gradual | informative) == informative)
    static_assert((informative | gradual) == informative)
    static_assert((gradual & informative) == informative)
    static_assert((informative & gradual) == informative)
```

We record constraints under all possible materializations of the gradual type, based on its upper
and lower bound.

```py
from collections.abc import Iterable
from typing import Any, Callable
from ty_extensions import Intersection, Top, Unknown, static_assert
from ty_extensions._internal import (
    ConstraintSet,
    is_constraint_set_assignable_to,
    is_subtype_of,
)

type LowerBounded = Iterable[str] | Any
type UpperBounded = Intersection[Any, Iterable[int]]
type Bounded = Iterable[bool] | Intersection[Any, Iterable[int]]
type InvariantUpperBounded = Intersection[Any, Top[list[Unknown]]]
type ConsumerLowerBounded = Callable[[str], None] | Any
type ConsumerUpperBounded = Intersection[Any, Callable[[int], None]]
type ConsumerBounded = Callable[[int], None] | Intersection[Any, Callable[[bool], None]]
type StableProjection = tuple[int, str] | Intersection[Any, tuple[int, object]]

def infer_from_source[T](value: Iterable[T]) -> T:
    raise NotImplementedError

def infer_from_target[T](value: Callable[[T], None]) -> T:
    raise NotImplementedError

def unbounded(source: Any, target: Callable[[Any], None]) -> None:
    reveal_type(infer_from_source(source))  # revealed: Any
    reveal_type(infer_from_target(target))  # revealed: Any

def bare_ranges[T]() -> None:
    source_unbounded = is_constraint_set_assignable_to(Any, T)
    source_lower = is_constraint_set_assignable_to(str | Any, T)
    source_upper = is_constraint_set_assignable_to(Intersection[Any, int], T)
    source_bounded = is_constraint_set_assignable_to(bool | Intersection[Any, int], T)
    target_unbounded = is_constraint_set_assignable_to(T, Any)
    target_lower = is_constraint_set_assignable_to(T, str | Any)
    target_upper = is_constraint_set_assignable_to(T, Intersection[Any, int])
    target_bounded = is_constraint_set_assignable_to(T, bool | Intersection[Any, int])

    # revealed: tuple[Solution[T=Any]]
    reveal_type(source_unbounded.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=str | Any]]
    reveal_type(source_lower.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any]]
    reveal_type(source_upper.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=bool | Any]]
    reveal_type(source_bounded.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any]]
    reveal_type(target_unbounded.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any]]
    reveal_type(target_lower.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any & int]]
    reveal_type(target_upper.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any & int]]
    reveal_type(target_bounded.solutions_for(T, inferable=tuple[T]))

def source_ranges[T]() -> None:
    lower = is_constraint_set_assignable_to(LowerBounded, Iterable[T])
    upper = is_constraint_set_assignable_to(UpperBounded, Iterable[T])
    bounded = is_constraint_set_assignable_to(Bounded, Iterable[T])
    invariant = is_constraint_set_assignable_to(InvariantUpperBounded, list[T])
    stable = is_constraint_set_assignable_to(StableProjection, tuple[T, object])

    # revealed: tuple[Solution[T=str | Any]]
    reveal_type(lower.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any]]
    reveal_type(upper.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=bool | Any]]
    reveal_type(bounded.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any]]
    reveal_type(invariant.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=int]]
    reveal_type(stable.solutions_for(T, inferable=tuple[T]))

def quantify_noninferable[T, U]() -> None:
    constraints = is_constraint_set_assignable_to(tuple[str, int] | Any, tuple[T, U])

    # revealed: tuple[Solution[T=str | Any]]
    reveal_type(constraints.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[U=int | Any]]
    reveal_type(constraints.solutions_for(U, inferable=tuple[U]))

def nested_ranges[T]() -> None:
    fixed_point = is_constraint_set_assignable_to(Any | str, T | int)
    nested = is_constraint_set_assignable_to(tuple[Any | str] | Any, tuple[T | int])
    recursive = is_constraint_set_assignable_to(Intersection[Any, list[int]], T | bytes)

    # revealed: tuple[Solution[T=str | Any]]
    reveal_type(fixed_point.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=str | Any]]
    reveal_type(nested.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any]]
    reveal_type(recursive.solutions_for(T, inferable=tuple[T]))

def target_ranges[T]() -> None:
    lower = is_constraint_set_assignable_to(Iterable[T], LowerBounded)
    upper = is_constraint_set_assignable_to(Iterable[T], UpperBounded)
    bounded = is_constraint_set_assignable_to(Iterable[T], Bounded)
    stable = is_constraint_set_assignable_to(tuple[T, str], StableProjection)

    # revealed: tuple[Solution[T=Any]]
    reveal_type(lower.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any & int]]
    reveal_type(upper.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any & int]]
    reveal_type(bounded.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=int]]
    reveal_type(stable.solutions_for(T, inferable=tuple[T]))

def contravariant_source_ranges[T]() -> None:
    lower = is_constraint_set_assignable_to(ConsumerLowerBounded, Callable[[T], None])
    upper = is_constraint_set_assignable_to(ConsumerUpperBounded, Callable[[T], None])
    bounded = is_constraint_set_assignable_to(ConsumerBounded, Callable[[T], None])

    # revealed: tuple[Solution[T=Any & str]]
    reveal_type(lower.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any]]
    reveal_type(upper.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=Any & int]]
    reveal_type(bounded.solutions_for(T, inferable=tuple[T]))

def contravariant_target_ranges[T]() -> None:
    lower = is_constraint_set_assignable_to(Callable[[T], None], ConsumerLowerBounded)
    upper = is_constraint_set_assignable_to(Callable[[T], None], ConsumerUpperBounded)
    bounded = is_constraint_set_assignable_to(Callable[[T], None], ConsumerBounded)

    # revealed: tuple[Solution[T=Any]]
    reveal_type(lower.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=int | Any]]
    reveal_type(upper.solutions_for(T, inferable=tuple[T]))
    # revealed: tuple[Solution[T=bool | Any]]
    reveal_type(bounded.solutions_for(T, inferable=tuple[T]))

# Subtyping still compares the static endpoints of a gradual range.
static_assert(is_subtype_of(Iterable[bool], Bounded))
static_assert(is_subtype_of(Bounded, Iterable[int]))
static_assert(is_subtype_of(UpperBounded, Iterable[int]))
static_assert(not is_subtype_of(LowerBounded, Iterable[object]))
static_assert(is_subtype_of(Any, object))
static_assert(not is_subtype_of(object, Any))

# Unsatisifable ranges collapse to never.
def unsatisfiable[T]() -> None:
    source = is_constraint_set_assignable_to(str | Any, list[T])
    target = is_constraint_set_assignable_to(list[T], Intersection[Any, int])
    static_assert(source == ConstraintSet.never())
    static_assert(target == ConstraintSet.never())
```

Constraint implication uses strict subtyping, and does not make assumptions about the
materialization of a given gradual type.

```py
from typing import Any, Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class A(Any): ...
class B(Any): ...

def _[T]() -> None:
    a = ConstraintSet.range(A, T, A)
    b = ConstraintSet.range(B, T, B)
    static_assert(not a.satisfies(b))
    static_assert(not b.satisfies(a))

def gradual_bounds_are_not_materialized[T]() -> None:
    gradual_lower = ConstraintSet.range(Any, T, object)
    int_lower = ConstraintSet.range(int, T, object)
    object_lower = ConstraintSet.range(object, T, object)
    static_assert(not int_lower.satisfies(gradual_lower))
    static_assert(not gradual_lower.satisfies(int_lower))
    static_assert(object_lower.satisfies(gradual_lower))
    static_assert((gradual_lower | object_lower) == gradual_lower)

    gradual_upper = ConstraintSet.range(Never, T, Any)
    int_upper = ConstraintSet.range(Never, T, int)
    static_assert(not int_upper.satisfies(gradual_upper))
    static_assert(not gradual_upper.satisfies(int_upper))
```

Unrelated instances of gradual types may materialize to distinct types, and so cannot establish
transitive subtyping relationships.

```py
from typing import Any, Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def independent_gradual_pivots[T, U]() -> None:
    constraints = ConstraintSet.range(Never, T, Any) & ConstraintSet.range(Any, U, object)
    static_assert(not constraints.implies_subtype_of(T, U))
```

## Displaying constraints

The `with_detailed_display` method can be used to print out the boolean formula that a constraint
set represents. However, this method is only intended for debugging purposes, and we reserve the
right to change the rendering at any time! We therefore do _not_ have a battery of mdtests printing
out all of the different kinds of constraints described above. Here we just test that the method
exists, and provides more detail than otherwise.

```py
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...

def _[T]() -> None:
    # revealed: ConstraintSet[bool]
    reveal_type(ConstraintSet.range(Sub, T, Super))
    # We are not asserting anything specific about what's displayed here, just that it's different
    # from above. If our constraint set rendering changes, update this accordingly.
    # revealed: ConstraintSet[(Sub ≤ T@_ ≤ Super)]
    reveal_type(ConstraintSet.range(Sub, T, Super).with_detailed_display())
```
