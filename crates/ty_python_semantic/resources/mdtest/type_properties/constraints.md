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
from ty_extensions import ConstraintSet, static_assert

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

Constraints can only refer to fully static types, so the lower and upper bounds are transformed into
their bottom and top materializations, respectively.

```py
def _[T]() -> None:
    constraints = ConstraintSet.range(Base, T, Any)
    expected = ConstraintSet.range(Base, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Sequence[Base], T, Sequence[Any])
    expected = ConstraintSet.range(Sequence[Base], T, Sequence[object])
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Any, T, Base)
    expected = ConstraintSet.range(Never, T, Base)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Sequence[Any], T, Sequence[Base])
    expected = ConstraintSet.range(Sequence[Never], T, Sequence[Base])
    static_assert(constraints == expected)
```

### Negated range

A _negated range_ constraint is the opposite of a range constraint: it requires the typevar to _not_
be within a particular lower and upper bound. The typevar can only specialize to a type that is a
strict subtype of the lower bound, a strict supertype of the upper bound, or incomparable to either.

```py
from typing import Any, final, Never, Sequence
from ty_extensions import ConstraintSet, Not, static_assert

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

```py
def _[T]() -> None:
    # ¬(T@_ ≤ Base)
    ~ConstraintSet.range(Never, T, Base)
```

Similarly, every type is a subtype of `object`, so an upper bound of `object` is the same as having
no upper bound.

```py
def _[T]() -> None:
    # ¬(Base ≤ T@_)
    ~ConstraintSet.range(Base, T, object)
```

And a negated range constraint with _both_ a lower bound of `Never` and an upper bound of `object`
cannot be satisfied at all.

```py
def _[T]() -> None:
    # (T@_ ≠ *)
    ~ConstraintSet.range(Never, T, object)
```

If the lower bound and upper bounds are "inverted" (the upper bound is a subtype of the lower bound)
or incomparable, then the negated range constraint can always be satisfied.

```py
def _[T]() -> None:
    static_assert(~ConstraintSet.range(Super, T, Sub))
    static_assert(~ConstraintSet.range(Base, T, Unrelated))
```

The lower and upper bound can be the same type, in which case the typevar can be specialized to any
type other than that specific type.

```py
def _[T]() -> None:
    # (T@_ ≠ Base)
    ~ConstraintSet.range(Base, T, Base)
```

Constraints can only refer to fully static types, so the lower and upper bounds are transformed into
their bottom and top materializations, respectively.

```py
def _[T]() -> None:
    constraints = ~ConstraintSet.range(Base, T, Any)
    expected = ~ConstraintSet.range(Base, T, object)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Sequence[Base], T, Sequence[Any])
    expected = ~ConstraintSet.range(Sequence[Base], T, Sequence[object])
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Any, T, Base)
    expected = ~ConstraintSet.range(Never, T, Base)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Sequence[Any], T, Sequence[Base])
    expected = ~ConstraintSet.range(Sequence[Never], T, Sequence[Base])
    static_assert(constraints == expected)
```

A negated _type_ is not the same thing as a negated _range_.

```py
def _[T]() -> None:
    negated_type = ConstraintSet.range(Never, T, Not[int])
    negated_constraint = ~ConstraintSet.range(Never, T, int)
    static_assert(negated_type != negated_constraint)
```

## Intersection

The intersection of two constraint sets requires that the constraints in both sets hold. In many
cases, we can simplify the result of an intersection.

### Different typevars

```py
from ty_extensions import ConstraintSet

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

```py
from typing import final
from ty_extensions import ConstraintSet, static_assert

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

```py
def _[T]() -> None:
    static_assert(not ConstraintSet.range(SubSub, T, Sub) & ConstraintSet.range(Base, T, Super))
    static_assert(not ConstraintSet.range(SubSub, T, Sub) & ConstraintSet.range(Unrelated, T, object))
```

Expanding on this, when intersecting two upper bounds constraints (`(T ≤ Base) ∧ (T ≤ Other)`), we
intersect the upper bounds. Any type that satisfies both `T ≤ Base` and `T ≤ Other` must necessarily
satisfy their intersection `T ≤ Base & Other`, and vice versa.

```py
from typing import Never
from ty_extensions import Intersection

# This is not final, so it's possible for a subclass to inherit from both Base and Other.
class Other: ...

def upper_bounds[T]():
    # (T@upper_bounds ≤ Base & Other)
    intersection_type = ConstraintSet.range(Never, T, Intersection[Base, Other])
    # (T@upper_bounds ≤ Base) ∧ (T@upper_bounds ≤ Other)
    intersection_constraint = ConstraintSet.range(Never, T, Base) & ConstraintSet.range(Never, T, Other)
    static_assert(intersection_type == intersection_constraint)
```

For an intersection of two lower bounds constraints (`(Base ≤ T) ∧ (Other ≤ T)`), we union the lower
bounds. Any type that satisfies both `Base ≤ T` and `Other ≤ T` must necessarily satisfy their union
`Base | Other ≤ T`, and vice versa.

```py
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
from ty_extensions import ConstraintSet, static_assert

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

Otherwise we clip the negative constraint to the mininum range that overlaps with the positive
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
from ty_extensions import ConstraintSet, static_assert

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
from ty_extensions import ConstraintSet

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
from ty_extensions import ConstraintSet, static_assert

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
from ty_extensions import ConstraintSet, static_assert

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

Otherwise we clip the positive constraint to the mininum range that overlaps with the negative
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
from ty_extensions import ConstraintSet, static_assert

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
from ty_extensions import ConstraintSet, static_assert

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
from ty_extensions import ConstraintSet, static_assert

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
from ty_extensions import ConstraintSet, static_assert

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

```py
from typing import Never
from ty_extensions import ConstraintSet, Intersection, static_assert

def f[T]():
    c1 = ConstraintSet.range(Never, T, str | int)
    c2 = ConstraintSet.range(Never, T, int | str)
    static_assert(c1 == c2)

    c1 = ConstraintSet.range(Never, T, Intersection[str, int])
    c2 = ConstraintSet.range(Never, T, Intersection[int, str])
    static_assert(c1 == c2)
```

### Constraints on the same typevar

Any particular specialization maps each typevar to one type. That means it's not useful to constrain
a typevar with itself as an upper or lower bound. No matter what type the typevar is specialized to,
that type is always a subtype of itself. (Remember that typevars are only specialized to fully
static types.)

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

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

```py
from ty_extensions import Intersection

def same_typevar[T]():
    constraints = ConstraintSet.range(Never, T, T | None)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Intersection[T, None], T, object)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Intersection[T, None], T, T | None)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)
```

Similarly, if the lower bound is an intersection containing the _negation_ of the typevar, then the
constraint set can never be satisfied, since every type is disjoint with its negation.

```py
from ty_extensions import Not

def same_typevar[T]():
    constraints = ConstraintSet.range(Intersection[Not[T], None], T, object)
    expected = ~ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Not[T], T, object)
    expected = ~ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)
```
