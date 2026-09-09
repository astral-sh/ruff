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
from typing import Any, Callable, final, Never, Sequence
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

Every type is a supertype of `Never`, so `upper_bound` can omit the lower bound.

```py
def _[T]() -> None:
    # (T@_ ≤ Base)
    ConstraintSet.upper_bound(T, Base)
```

Similarly, every type is a subtype of `object`, so `lower_bound` can omit the upper bound.

```py
def _[T]() -> None:
    # (Base ≤ T@_)
    ConstraintSet.lower_bound(Base, T)
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

Ordinary TypeVar bounds compare whole callables, so incompatible returns make a range unsatisfiable.

```py
def callable_returns[T]() -> None:
    static_assert(ConstraintSet.range(Callable[[int], int], T, Callable[[int], str]) == ConstraintSet.never())
```

When the lower and upper bounds are the same type, `equality` requires the typevar to specialize to
that specific type.

```py
def _[T]() -> None:
    # (T@_ = Base)
    ConstraintSet.equality(T, Base)
```

Constraints can only refer to fully static types, so the lower and upper bounds are transformed into
their bottom and top materializations, respectively.

```py
def _[T]() -> None:
    constraints = ConstraintSet.range(Base, T, Any)
    expected = ConstraintSet.lower_bound(Base, T)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Sequence[Base], T, Sequence[Any])
    expected = ConstraintSet.range(Sequence[Base], T, Sequence[object])
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Any, T, Base)
    expected = ConstraintSet.upper_bound(T, Base)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Sequence[Any], T, Sequence[Base])
    expected = ConstraintSet.range(Sequence[Never], T, Sequence[Base])
    static_assert(constraints == expected)
```

### Lower bound

A lower-bound constraint requires the type variable to be a supertype of its bound without providing
upper-bound evidence.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet, is_constraint_set_assignable_to

def _[T]() -> None:
    expected = is_constraint_set_assignable_to(int, T)
    static_assert(ConstraintSet.lower_bound(int, T) == expected)
```

Ordinary TypeVar bounds retain callable returns.

```py
from typing import Callable

def callable_bound[T]() -> None:
    constraints = ConstraintSet.lower_bound(Callable[[int], int], T)
    static_assert(constraints != ConstraintSet.lower_bound(Callable[[int], str], T))
```

### Upper bound

An upper-bound constraint requires the type variable to be a subtype of its bound without providing
lower-bound evidence.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet, is_constraint_set_assignable_to

def _[T]() -> None:
    expected = is_constraint_set_assignable_to(T, int)
    static_assert(ConstraintSet.upper_bound(T, int) == expected)
```

Upper bounds likewise retain ordinary callable returns.

```py
from typing import Callable

def callable_bound[T]() -> None:
    constraints = ConstraintSet.upper_bound(T, Callable[[int], int])
    static_assert(constraints != ConstraintSet.upper_bound(T, Callable[[int], str]))
```

Unlike an explicit two-sided range, an upper-bound constraint does not supply `Never` as lower-bound
inference evidence.

```py
from typing import Never

def inferred_solution[T]() -> None:
    # revealed: tuple[Solution[T=int]]
    reveal_type(ConstraintSet.upper_bound(T, int).solutions_for(T, inferable=tuple[T]))

    # revealed: tuple[Solution[T=Never]]
    reveal_type(ConstraintSet.range(Never, T, int).solutions_for(T, inferable=tuple[T]))
```

### Equality

An equality constraint requires the type variable to specialize exactly to the specified type. It is
equivalent to an explicit range with that type as both bounds.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def _[T]() -> None:
    equality = ConstraintSet.equality(T, int)
    static_assert(equality == ConstraintSet.range(int, T, int))

    # revealed: tuple[Solution[T=int]]
    reveal_type(equality.solutions_for(T, inferable=tuple[T]))
```

Equality of ordinary types also includes callable returns.

```py
from typing import Callable

def callable_bound[T]() -> None:
    constraints = ConstraintSet.equality(T, Callable[[int], int])
    static_assert(constraints != ConstraintSet.equality(T, Callable[[int], str]))
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

Every type is a supertype of `Never`, so `upper_bound` can omit the lower bound.

```pyi
def _[T]() -> None:
    # ¬(T@_ ≤ Base)
    ~ConstraintSet.upper_bound(T, Base)
```

Similarly, every type is a subtype of `object`, so `lower_bound` can omit the upper bound.

```pyi
def _[T]() -> None:
    # ¬(Base ≤ T@_)
    ~ConstraintSet.lower_bound(Base, T)
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
    ~ConstraintSet.equality(T, Base)
```

Constraints can only refer to fully static types, so the lower and upper bounds are transformed into
their bottom and top materializations, respectively.

```pyi
def _[T]() -> None:
    constraints = ~ConstraintSet.range(Base, T, Any)
    expected = ~ConstraintSet.lower_bound(Base, T)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Sequence[Base], T, Sequence[Any])
    expected = ~ConstraintSet.range(Sequence[Base], T, Sequence[object])
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Any, T, Base)
    expected = ~ConstraintSet.upper_bound(T, Base)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Sequence[Any], T, Sequence[Base])
    expected = ~ConstraintSet.range(Sequence[Never], T, Sequence[Base])
    static_assert(constraints == expected)
```

A negated _type_ is not the same thing as a negated _range_.

```pyi
def _[T]() -> None:
    negated_type = ConstraintSet.upper_bound(T, ~int)
    negated_constraint = ~ConstraintSet.upper_bound(T, int)
    static_assert(negated_type != negated_constraint)
```

## Constraints from materialized types

### Invariant classes

Assignability between fully static specializations of an invariant class determines the type
variable exactly.

```py
from typing import Any
from ty_extensions import Bottom, Top
from ty_extensions._internal import is_constraint_set_assignable_to

class Invariant[T]:
    value: T

def inspect_exact[T]() -> None:
    exact = is_constraint_set_assignable_to(Top[Invariant[str]], Top[Invariant[T]])
    reveal_type(exact.solutions_for(T, inferable=tuple[T]))  # revealed: tuple[Solution[T=str]]
```

The top materialization of `Invariant[Any]` covers every static specialization represented by `Any`.
No single fully static specialization of `T` can cover that range. The reverse
bottom-materialization comparison is impossible for the same reason. Constraint inference preserves
both ends of those ranges, so neither comparison has a solution.

```py
def inspect_gradual[T]() -> None:
    top = is_constraint_set_assignable_to(Top[Invariant[Any]], Top[Invariant[T]])
    reveal_type(top.solutions_for(T, inferable=tuple[T]))  # revealed: None

    bottom = is_constraint_set_assignable_to(Bottom[Invariant[T]], Bottom[Invariant[Any]])
    reveal_type(bottom.solutions_for(T, inferable=tuple[T]))  # revealed: None
```

### Recursive consuming methods

A recursive consuming method imposes the opposite constraint from a covariant property. Seeing the
type variable in the property's constraints is not enough to omit that method: both bounds are
needed to establish the invariant specialization. The `Any` marker keeps the protocol gradual even
when its type argument is fully static.

```py
from __future__ import annotations

from typing import Any, Protocol
from ty_extensions import Top, static_assert
from ty_extensions._internal import ConstraintSet, is_constraint_set_assignable_to

class RecursiveInvariant[T](Protocol):
    marker: Any

    @property
    def value(self) -> T: ...
    def consume(self, other: RecursiveInvariant[T]) -> None: ...

def inspect[T]() -> None:
    constraints = is_constraint_set_assignable_to(Top[RecursiveInvariant[str]], Top[RecursiveInvariant[T]])
    static_assert(constraints == ConstraintSet.equality(T, str))
```

A top-materialized `Any` also contributes both bounds. Its recursive consuming requirement makes the
relation impossible for every fully static specialization of `T`.

```py
def inspect_gradual[T]() -> None:
    constraints = is_constraint_set_assignable_to(Top[RecursiveInvariant[Any]], Top[RecursiveInvariant[T]])
    reveal_type(constraints.solutions_for(T, inferable=tuple[T]))  # revealed: None
```

### Constraints introduced by recursive properties

A recursive property can introduce constraints that the outer properties do not impose. Here, the
outer `value` properties both return `str | int`, but the children return `bytes | int` and
`T | int`. The child therefore contributes the lower bound `bytes <: T`. Its specialization stays
unchanged on subsequent recursive steps.

```py
from __future__ import annotations

from typing import Protocol
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import ConstraintSet, is_constraint_set_assignable_to

class Recursive[A, B](Protocol):
    @property
    def value(self) -> A | int: ...
    @property
    def child(self) -> Recursive[B, B]: ...

def materialized_source[T]() -> None:
    top = is_constraint_set_assignable_to(Top[Recursive[str | int, bytes]], Recursive[str, T])
    static_assert(top == ConstraintSet.lower_bound(bytes, T))
    reveal_type(top.solutions_for(T, inferable=tuple[T]))  # revealed: tuple[Solution[T=bytes]]

    bottom = is_constraint_set_assignable_to(Bottom[Recursive[str | int, bytes]], Recursive[str, T])
    static_assert(bottom == ConstraintSet.lower_bound(bytes, T))
```

Materializing the target instead preserves the same bound. These specializations contain no gradual
types, so neither materialization changes their requirements.

```py
def materialized_target[T]() -> None:
    top = is_constraint_set_assignable_to(Recursive[str | int, bytes], Top[Recursive[str, T]])
    static_assert(top == ConstraintSet.lower_bound(bytes, T))

    bottom = is_constraint_set_assignable_to(Recursive[str | int, bytes], Bottom[Recursive[str, T]])
    static_assert(bottom == ConstraintSet.lower_bound(bytes, T))
```

The bound also survives opposite materializations. Combining it with an incompatible upper bound has
no solution; the recursive comparison does not merely succeed without constraining `T`.

```py
def incompatible_bound[T]() -> None:
    constraints = is_constraint_set_assignable_to(Top[Recursive[str | int, bytes]], Bottom[Recursive[str, T]])
    static_assert(constraints == ConstraintSet.lower_bound(bytes, T))

    incompatible = constraints & ConstraintSet.upper_bound(T, str)
    reveal_type(incompatible.solutions_for(T, inferable=tuple[T]))  # revealed: None
```

### Opposite materializations of recursive protocols

A fixed `Any` in a recursive method changes independently of the protocol's type parameter. The
top-materialized return type `object` cannot satisfy the bottom-materialized return requirement
`Never`. The matching nonrecursive property can constrain `T`, but no specialization satisfies the
complete protocol.

```py
from __future__ import annotations

from typing import Any, Protocol
from ty_extensions import Bottom, Top
from ty_extensions._internal import is_constraint_set_assignable_to

class RecursiveValue[T](Protocol):
    @property
    def value(self) -> T: ...
    def consume(self, child: RecursiveValue[Any]) -> Any: ...

def inspect[T]() -> None:
    constraints = is_constraint_set_assignable_to(Top[RecursiveValue[str]], Bottom[RecursiveValue[T]])
    reveal_type(constraints.solutions_for(T, inferable=tuple[T]))  # revealed: None
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
    expected = ConstraintSet.equality(T, Base)
    static_assert(constraints == expected)

    constraints = ConstraintSet.range(Sub, T, Super) & ConstraintSet.range(Sub, T, Super)
    expected = ConstraintSet.range(Sub, T, Super)
    static_assert(constraints == expected)
```

If they don't overlap, the intersection is empty.

```pyi
def _[T]() -> None:
    static_assert(not ConstraintSet.range(SubSub, T, Sub) & ConstraintSet.range(Base, T, Super))
    static_assert(not ConstraintSet.range(SubSub, T, Sub) & ConstraintSet.lower_bound(Unrelated, T))
```

Expanding on this, when intersecting two upper bounds constraints (`(T ≤ Base) ∧ (T ≤ Other)`), we
intersect the upper bounds. Any type that satisfies both `T ≤ Base` and `T ≤ Other` must necessarily
satisfy their intersection `T ≤ Base & Other`, and vice versa.

```pyi
# This is not final, so it's possible for a subclass to inherit from both Base and Other.
class Other: ...

def upper_bounds[T]():
    # (T@upper_bounds ≤ Base & Other)
    intersection_type = ConstraintSet.upper_bound(T, Base & Other)
    # (T@upper_bounds ≤ Base) ∧ (T@upper_bounds ≤ Other)
    intersection_constraint = ConstraintSet.upper_bound(T, Base) & ConstraintSet.upper_bound(T, Other)
    static_assert(intersection_type == intersection_constraint)
```

For an intersection of two lower bounds constraints (`(Base ≤ T) ∧ (Other ≤ T)`), we union the lower
bounds. Any type that satisfies both `Base ≤ T` and `Other ≤ T` must necessarily satisfy their union
`Base | Other ≤ T`, and vice versa.

```pyi
def lower_bounds[T]():
    # (Base | Other ≤ T@lower_bounds)
    union_type = ConstraintSet.lower_bound(Base | Other, T)
    # (Base ≤ T@upper_bounds) ∧ (Other ≤ T@upper_bounds)
    intersection_constraint = ConstraintSet.lower_bound(Base, T) & ConstraintSet.lower_bound(Other, T)
    static_assert(union_type == intersection_constraint)
```

### Intersection of two equality constraints

A type variable cannot be exactly equal to two non-equivalent fully static types. This is stronger
than checking whether the types are disjoint: two classes can have a common subclass, which makes
their upper-bound constraints compatible, but that subclass is not exactly equal to either class.

Gradual bounds cannot prove this incompatibility. Sequent maps derive facts via transitivity, but
gradual assignability is not transitive. That means equality constraints containing dynamic types
remain conservatively satisfiable. Type variables nested inside a bound are treated as opaque
symbolic atoms; their declared bounds do not make an otherwise static proof gradual.

```py
from typing import Any
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Left: ...
class Right: ...
class Both(Left, Right): ...

def _[T, U: Any, V]() -> None:
    left = ConstraintSet.equality(T, Left)
    right = ConstraintSet.equality(T, Right)
    static_assert(~(left & right))

    equivalent = left & left
    static_assert(equivalent == left)

    upper_bounds = ConstraintSet.upper_bound(T, Left) & ConstraintSet.upper_bound(T, Right)
    static_assert(not ~upper_bounds)

    both = ConstraintSet.equality(T, Both)
    static_assert(both & upper_bounds == both)

    symbolic_static_mismatch = ConstraintSet.equality(T, tuple[U, int]) & ConstraintSet.equality(T, tuple[U, str])
    static_assert(~symbolic_static_mismatch)

    gradual_mismatch = ConstraintSet.equality(T, list[Any]) & ConstraintSet.equality(T, list[int])
    static_assert(not ~gradual_mismatch)

    any_mismatch = ConstraintSet.equality(T, Any) & ConstraintSet.equality(T, int)
    static_assert(not ~any_mismatch)

    symbolic_gradual_mismatch = ConstraintSet.equality(T, tuple[U, Any]) & ConstraintSet.equality(T, tuple[U, int])
    static_assert(not ~symbolic_gradual_mismatch)

    symbolic_match = ConstraintSet.equality(T, list[U]) & ConstraintSet.equality(T, list[V])
    static_assert(not ~symbolic_match)
```

### Intersection of a range and a negated range

The bounds of the range constraint provide a range of types that should be included; the bounds of
the negated range constraint provide a "hole" of types that should not be included. We can think of
the intersection as removing the hole from the range constraint.

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
    constraints = ConstraintSet.range(Sub, T, Base) & ~ConstraintSet.upper_bound(T, Unrelated)
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
    ~ConstraintSet.range(SubSub, T, Sub) & ~ConstraintSet.lower_bound(Unrelated, T)
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
    ConstraintSet.range(SubSub, T, Sub) | ConstraintSet.lower_bound(Unrelated, T)
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
# This is not final, so it's possible for a subclass to inherit from both Base and Other.
class Other: ...

def union[T]():
    # (T@union ≤ Base | Other)
    union_type = ConstraintSet.upper_bound(T, Base | Other)
    # (T@union ≤ Base) ∨ (T@union ≤ Other)
    union_constraint = ConstraintSet.upper_bound(T, Base) | ConstraintSet.upper_bound(T, Other)

    # (T = Base | Other) satisfies (T ≤ Base | Other) but not (T ≤ Base ∨ T ≤ Other)
    specialization = ConstraintSet.equality(T, Base | Other)
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
    union_type = ConstraintSet.lower_bound(Base | Other, T)
    # (Base ≤ T@union) ∨ (Other ≤ T@union)
    union_constraint = ConstraintSet.lower_bound(Base, T) | ConstraintSet.lower_bound(Other, T)

    # (T = Base) satisfies (Base ≤ T ∨ Other ≤ T) but not (Base | Other ≤ T)
    specialization = ConstraintSet.equality(T, Base)
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
from typing import final
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
    constraints = ~ConstraintSet.range(Sub, T, Base) | ConstraintSet.upper_bound(T, Unrelated)
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
    expected = ~ConstraintSet.equality(T, Base)
    static_assert(constraints == expected)

    constraints = ~ConstraintSet.range(Sub, T, Super) | ~ConstraintSet.range(Sub, T, Super)
    expected = ~ConstraintSet.range(Sub, T, Super)
    static_assert(constraints == expected)
```

If the holes don't overlap, the union is always satisfied.

```py
def _[T]() -> None:
    static_assert(~ConstraintSet.range(SubSub, T, Sub) | ~ConstraintSet.range(Base, T, Super))
    static_assert(~ConstraintSet.range(SubSub, T, Sub) | ~ConstraintSet.lower_bound(Unrelated, T))
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
    ~ConstraintSet.upper_bound(T, Base)
    # ¬(Sub ≤ T@_)
    ~ConstraintSet.lower_bound(Sub, T)
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
from typing import final
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Base: ...

@final
class Unrelated: ...

def _[T, U]() -> None:
    # ¬(T@_ ≤ Base) ∨ ¬(U@_ ≤ Base)
    ~(ConstraintSet.upper_bound(T, Base) & ConstraintSet.upper_bound(U, Base))
```

The union of a constraint and its negation should always be satisfiable.

```py
def _[T, U]() -> None:
    c1 = ConstraintSet.upper_bound(T, Base) & ConstraintSet.upper_bound(U, Base)
    static_assert(c1 | ~c1)
    static_assert(~c1 | c1)

    c2 = ConstraintSet.lower_bound(Unrelated, T) & ConstraintSet.lower_bound(Unrelated, U)
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
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def f[S, T]():
    # (S@f ≤ T@f)
    c1 = ConstraintSet.upper_bound(S, T)
    c2 = ConstraintSet.lower_bound(S, T)
    static_assert(c1 == c2)

def f[T, S]():
    # (S@f ≤ T@f)
    c1 = ConstraintSet.upper_bound(S, T)
    c2 = ConstraintSet.lower_bound(S, T)
    static_assert(c1 == c2)
```

Equivalence constraints are similar; internally we arbitrarily choose the "earlier" typevar to be
the constraint, and the other the bound.

```py
def f[S, T]():
    # (S@f = T@f)
    c1 = ConstraintSet.equality(S, T)
    c2 = ConstraintSet.equality(T, S)
    static_assert(c1 == c2)

def f[T, S]():
    # (S@f = T@f)
    c1 = ConstraintSet.equality(S, T)
    c2 = ConstraintSet.equality(T, S)
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
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def f[T]():
    c1 = ConstraintSet.upper_bound(T, str | int)
    c2 = ConstraintSet.upper_bound(T, int | str)
    static_assert(c1 == c2)

    c1 = ConstraintSet.upper_bound(T, str & int)
    c2 = ConstraintSet.upper_bound(T, int & str)
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
    constraints = ConstraintSet.upper_bound(T, T)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.lower_bound(T, T)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.equality(T, T)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)
```

This is also true when the typevar appears in a union in the upper bound, or in an intersection in
the lower bound. (Note that this lines up with how we simplify the intersection of two constraints,
as shown above.)

```pyi
def same_typevar[T]():
    constraints = ConstraintSet.upper_bound(T, T | None)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.lower_bound(T & None, T)
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
    constraints = ConstraintSet.lower_bound(~T & None, T)
    expected = ~ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = ConstraintSet.lower_bound(~T, T)
    expected = ~ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)
```

Constraining a ParamSpec with itself leaves every parameter list possible.

```pyi
from typing import Callable
from ty_extensions import Bottom, Top

def same_paramspec[**P]() -> None:
    constraints = ConstraintSet.upper_bound(P, P)
    expected = ConstraintSet.range(Bottom[Callable[..., Never]], P, Top[Callable[..., object]])
    static_assert(constraints == expected)

    constraints = ConstraintSet.lower_bound(P, P)
    expected = ConstraintSet.range(Bottom[Callable[..., Never]], P, Top[Callable[..., object]])
    static_assert(constraints == expected)

    constraints = ConstraintSet.equality(P, P)
    expected = ConstraintSet.range(Bottom[Callable[..., Never]], P, Top[Callable[..., object]])
    static_assert(constraints == expected)
```

## Existential quantification

Existential quantification removes the listed typevars from a constraint set. Any constraints that
do not involve those typevars must remain in the result. The result holds whenever _at least one_
valid assignment to the quantified variables satisfies the expression being quantified over.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def preserves_remaining_conjunct[T, U]() -> None:
    t_int = ConstraintSet.equality(T, int)
    u_str = ConstraintSet.equality(U, str)
    quantified = (t_int & u_str).exists(tuple[U])
    static_assert(quantified == t_int)

def satisfies_uncertain_disjunct[T, U]() -> None:
    t_int = ConstraintSet.equality(T, int)
    u_str = ConstraintSet.equality(U, str)
    quantified = (t_int | u_str).exists(tuple[U])
    static_assert(quantified == ConstraintSet.always())

def no_typevars_is_identity[T]() -> None:
    constraints = ConstraintSet.upper_bound(T, int)
    static_assert(constraints.exists(tuple[()]) == constraints)
```

## Universal quantification

Universal quantification removes the listed typevars from a constraint set. Any constraints that do
not involve those typevars must remain in the result. The result holds whenever _every_ valid
assignment to the quantified variables satisfies the expression being quantified over.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def preserves_uncertain_disjunct[T, U]() -> None:
    t_int = ConstraintSet.equality(T, int)
    u_str = ConstraintSet.equality(U, str)
    quantified = (t_int | u_str).for_all(tuple[U])
    static_assert(quantified == t_int)

def removes_multiple_typevars[T, U]() -> None:
    t_int = ConstraintSet.equality(T, int)
    u_str = ConstraintSet.equality(U, str)
    quantified = (t_int | u_str).for_all(tuple[T, U])
    static_assert(quantified == ConstraintSet.never())

def no_typevars_is_identity[T]() -> None:
    constraints = ConstraintSet.upper_bound(T, int)
    static_assert(constraints.for_all(tuple[()]) == constraints)
```

The order of existential and universal quantifiers matters. For each target truth assignment there
is some matching source truth assignment, but no single source truth assignment matches every target
truth assignment.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def quantifier_order[S, T]() -> None:
    source_is_int = ConstraintSet.equality(S, int)
    target_is_int = ConstraintSet.equality(T, int)
    equal = source_is_int.satisfies(target_is_int) & target_is_int.satisfies(source_is_int)

    # ∀T.∃S.equal(S, T)
    forall_target_exists_source = equal.exists(tuple[S]).for_all(tuple[T])
    static_assert(forall_target_exists_source == ConstraintSet.always())

    # ∃S.∀T.equal(S, T)
    exists_source_forall_target = equal.for_all(tuple[T]).exists(tuple[S])
    static_assert(exists_source_forall_target == ConstraintSet.never())
```

## ParamSpec

A ParamSpec constraint describes parameter lists; callable returns are ignored.

### Construction

Legacy ParamSpecs work with every constructor.

```py
from typing import Callable, ParamSpec
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet, is_constraint_set_assignable_to

P = ParamSpec("P")

def legacy_range(callback: Callable[P, None]) -> None:
    constraints = ConstraintSet.range(Callable[[int, str], None], P, Callable[[int, str], None])
    different_returns = ConstraintSet.range(Callable[[int, str], int], P, Callable[[int, str], str])
    static_assert(constraints == different_returns)

def legacy_lower_bound(callback: Callable[P, None]) -> None:
    expected = is_constraint_set_assignable_to(Callable[[int, str], None], Callable[P, None])
    static_assert(ConstraintSet.lower_bound(Callable[[int, str], int], P) == expected)

def legacy_upper_bound(callback: Callable[P, None]) -> None:
    expected = is_constraint_set_assignable_to(Callable[P, None], Callable[[int, str], None])
    static_assert(ConstraintSet.upper_bound(P, Callable[[int, str], str]) == expected)

def legacy_equality(callback: Callable[P, None]) -> None:
    equality = ConstraintSet.equality(P, Callable[[int, str], bytes])
    static_assert(equality == ConstraintSet.range(Callable[[int, str], None], P, Callable[[int, str], None]))
```

An empty parameter list is an exact bound, distinct from a one-parameter list.

```py
def empty[**P]() -> None:
    constraints = ConstraintSet.range(Callable[[], None], P, Callable[[], None])
    static_assert(constraints != ConstraintSet.range(Callable[[int], None], P, Callable[[int], None]))
```

An alias of a known constructor retains its ParamSpec argument rules.

```py
def aliased_constructor[**P]() -> None:
    equals = ConstraintSet.equality
    constraints = equals(P, Callable[[int], None])
    static_assert(constraints == ConstraintSet.range(Callable[[int], None], P, Callable[[int], None]))
```

### Callable aliases

Specialized callable aliases have the same bounds as their expanded parameter lists.

```py
from typing import Callable, Concatenate
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

type Callback[**Q, R] = Callable[Q, R]

def aliases[**P]() -> None:
    constraints = ConstraintSet.range(Callback[[int, str, bool], int], P, Callback[[int, str, bool], str])
    expected = ConstraintSet.range(Callable[[int, str, bool], None], P, Callable[[int, str, bool], None])
    static_assert(constraints == expected)
```

Fully specializing a `Concatenate` alias preserves every prefix parameter and the concrete tail.

```py
type Prefixed[**Q, R] = Callable[Concatenate[int, str, Q], R]

def concatenate[**P]() -> None:
    constraints = ConstraintSet.range(Prefixed[[bool], int], P, Prefixed[[bool], str])
    expected = ConstraintSet.range(Callable[[int, str, bool], None], P, Callable[[int, str, bool], None])
    static_assert(constraints == expected)
```

### Two-sided bounds

A callable accepting `Super` and a consumer passing a `Sub` give `(Super, /) ≤ P ≤ (Sub, /)`.

```py
from typing import Callable, final
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def two_sided[**P]() -> None:
    constraints = ConstraintSet.range(Callable[[Super], None], P, Callable[[Sub], None])
    lower = ConstraintSet.lower_bound(Callable[[Super], int], P)
    upper = ConstraintSet.upper_bound(P, Callable[[Sub], str])
    static_assert(constraints == (lower & upper))
    static_assert(constraints != lower)
    static_assert(constraints != upper)
```

Inverted or incomparable bounds are unsatisfiable.

```py
def incompatible[**P]() -> None:
    inverted = ConstraintSet.range(Callable[[Sub], None], P, Callable[[Super], None])
    static_assert(inverted == ConstraintSet.never())
    incomparable = ConstraintSet.range(Callable[[Base], None], P, Callable[[Unrelated], None])
    static_assert(incomparable == ConstraintSet.never())
```

Individually satisfiable lower and upper bounds can have an empty intersection.

```py
def incompatible_intersection[**P]() -> None:
    lower = ConstraintSet.lower_bound(Callable[[Sub], None], P)
    upper = ConstraintSet.upper_bound(P, Callable[[Super], None])
    static_assert((lower & upper) == ConstraintSet.never())
```

### Symbolic bounds

Two ParamSpecs can be constrained to the same parameter list, in either order.

```py
from typing import Any, Callable
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet, is_constraint_set_assignable_to

def equality[**P, **Q]() -> None:
    constraints = ConstraintSet.equality(P, Q)
    expected = is_constraint_set_assignable_to(Callable[P, Any], Callable[Q, Any])
    static_assert(constraints == expected)
    static_assert(ConstraintSet.equality(Q, P) == constraints)
```

Each endpoint is retained when a symbolic lower bound is combined with a concrete upper bound.

```py
def symbolic_lower[**P, **Q]() -> None:
    constraints = ConstraintSet.range(Q, P, Callable[[int], None])
    lower = ConstraintSet.lower_bound(Q, P)
    upper = ConstraintSet.upper_bound(P, Callable[[int], None])
    static_assert(constraints == (lower & upper))
    static_assert(constraints != lower)
    static_assert(constraints != upper)
```

Symbolic upper bounds likewise retain their concrete lower bound.

```py
def symbolic_upper[**P, **Q]() -> None:
    constraints = ConstraintSet.range(Callable[[int], None], P, Q)
    lower = ConstraintSet.lower_bound(Callable[[int], None], P)
    upper = ConstraintSet.upper_bound(P, Q)
    static_assert(constraints == (lower & upper))
    static_assert(constraints != lower)
    static_assert(constraints != upper)
```

Three ParamSpecs form a two-sided range.

```py
def symbolic_range[**P, **Q, **R]() -> None:
    constraints = ConstraintSet.range(Q, P, R)
    lower = ConstraintSet.lower_bound(Q, P)
    upper = ConstraintSet.upper_bound(P, R)
    static_assert(constraints == (lower & upper))
    static_assert(constraints != lower)
    static_assert(constraints != upper)
```

### Symbolic callable bounds

An unprefixed callable bound describes the same parameter list as its bare ParamSpec.

```py
from typing import Any, Callable, Concatenate
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet, is_constraint_set_assignable_to

def unprefixed[**P, **Q]() -> None:
    constraints = ConstraintSet.range(Callable[Q, int], P, Callable[Q, str])
    static_assert(constraints == ConstraintSet.range(Q, P, Q))
```

A `Concatenate` bound preserves its prefix and symbolic tail while erasing the return.

```py
def prefixed[**P, **Q]() -> None:
    constraints = ConstraintSet.range(Callable[Concatenate[int, Q], int], P, Callable[Concatenate[int, Q], str])
    expected = is_constraint_set_assignable_to(Callable[Concatenate[int, Q], int], Callable[P, Any])
    expected &= is_constraint_set_assignable_to(Callable[P, Any], Callable[Concatenate[int, Q], str])
    static_assert(constraints == expected)
    static_assert(constraints != ConstraintSet.range(Q, P, Q))
    different_prefix = ConstraintSet.range(Callable[Concatenate[str, Q], None], P, Callable[Concatenate[str, Q], None])
    static_assert(constraints != different_prefix)
```

### Signature preservation

Named parameters accept positional-only calls; the reverse range is invalid.

```pyi
from typing import Callable
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet, RegularCallableTypeOf

def named(value: int) -> None: ...
def positional_only[**P]() -> None:
    constraints = ConstraintSet.range(RegularCallableTypeOf[named], P, Callable[[int], None])
    static_assert(constraints != ConstraintSet.never())
    reverse = ConstraintSet.range(Callable[[int], None], P, RegularCallableTypeOf[named])
    static_assert(reverse == ConstraintSet.never())
```

Named parameters also accept keyword-only calls; the reverse range is invalid.

```pyi
def keyword(*, value: int) -> None: ...
def keyword_only[**P]() -> None:
    constraints = ConstraintSet.range(RegularCallableTypeOf[named], P, RegularCallableTypeOf[keyword])
    static_assert(constraints != ConstraintSet.never())
    reverse = ConstraintSet.range(RegularCallableTypeOf[keyword], P, RegularCallableTypeOf[named])
    static_assert(reverse == ConstraintSet.never())
```

An optional parameter accepts every call to a required parameter, but not the reverse.

```pyi
def optional(value: int = ...) -> None: ...
def defaults[**P]() -> None:
    constraints = ConstraintSet.range(RegularCallableTypeOf[optional], P, RegularCallableTypeOf[named])
    static_assert(constraints != ConstraintSet.never())
    reverse = ConstraintSet.range(RegularCallableTypeOf[named], P, RegularCallableTypeOf[optional])
    static_assert(reverse == ConstraintSet.never())
```

Variadic positional parameters accept fixed positional lists, but not the reverse.

```pyi
def args(*args: int) -> None: ...
def positional_variadics[**P]() -> None:
    constraints = ConstraintSet.range(RegularCallableTypeOf[args], P, Callable[[int, int], None])
    static_assert(constraints != ConstraintSet.never())
    reverse = ConstraintSet.range(Callable[[int, int], None], P, RegularCallableTypeOf[args])
    static_assert(reverse == ConstraintSet.never())
```

Variadic keyword parameters likewise accept a fixed keyword-only parameter, but not the reverse.

```pyi
def kwargs(**kwargs: int) -> None: ...
def keyword_variadics[**P]() -> None:
    constraints = ConstraintSet.range(RegularCallableTypeOf[kwargs], P, RegularCallableTypeOf[keyword])
    static_assert(constraints != ConstraintSet.never())
    reverse = ConstraintSet.range(RegularCallableTypeOf[keyword], P, RegularCallableTypeOf[kwargs])
    static_assert(reverse == ConstraintSet.never())
```

### Overloaded bounds

Return types are erased in every overload, without keeping only the first or last parameter list.

```pyi
from typing import Callable, overload
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet, RegularCallableTypeOf

@overload
def overloaded(value: int, /) -> int: ...
@overload
def overloaded(*, value: str) -> str: ...
@overload
def swapped_returns(value: int, /) -> str: ...
@overload
def swapped_returns(*, value: str) -> int: ...
def keyword(*, value: str) -> None: ...
def overloads[**P]() -> None:
    constraints = ConstraintSet.range(RegularCallableTypeOf[overloaded], P, RegularCallableTypeOf[swapped_returns])
    static_assert(constraints == ConstraintSet.range(RegularCallableTypeOf[overloaded], P, RegularCallableTypeOf[overloaded]))
    static_assert(constraints != ConstraintSet.range(Callable[[int], None], P, Callable[[int], None]))
    static_assert(constraints != ConstraintSet.range(RegularCallableTypeOf[keyword], P, RegularCallableTypeOf[keyword]))
```

An overloaded lower bound can satisfy a single signature; an overloaded upper bound requires both.

```pyi
def asymmetric[**P]() -> None:
    constraints = ConstraintSet.range(RegularCallableTypeOf[overloaded], P, Callable[[int], None])
    static_assert(constraints != ConstraintSet.never())
    reverse = ConstraintSet.range(Callable[[int], None], P, RegularCallableTypeOf[overloaded])
    static_assert(reverse == ConstraintSet.never())
```

The string overload accepts only a keyword argument, not a positional argument.

```pyi
def parameter_kinds[**P]() -> None:
    constraints = ConstraintSet.range(RegularCallableTypeOf[overloaded], P, RegularCallableTypeOf[keyword])
    static_assert(constraints != ConstraintSet.never())
    positional = ConstraintSet.range(RegularCallableTypeOf[overloaded], P, Callable[[str], None])
    static_assert(positional == ConstraintSet.never())
```

### Gradual parameter lists

An empty parameter list is compatible with ellipsis, but not with one required `Any` parameter.

```py
from typing import Any, Callable
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def empty[**P]() -> None:
    static_assert(ConstraintSet.range(Callable[..., None], P, Callable[[], None]) != ConstraintSet.never())
    static_assert(ConstraintSet.range(Callable[[], None], P, Callable[..., None]) != ConstraintSet.never())
    static_assert(ConstraintSet.range(Callable[[Any], None], P, Callable[[], None]) == ConstraintSet.never())
    static_assert(ConstraintSet.range(Callable[[], None], P, Callable[[Any], None]) == ConstraintSet.never())
```

### Missing bounds

A missing lower bound is equivalent to the bottom signature, which accepts all arguments.

```py
from typing import Callable, Never
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import ConstraintSet

def missing_lower_bound[**P]() -> None:
    constraints = ConstraintSet.upper_bound(P, Callable[[int], int])
    expected = ConstraintSet.range(Bottom[Callable[..., Never]], P, Callable[[int], int])
    static_assert(constraints == expected)
```

A missing upper bound is equivalent to the top signature, which accepts no calls.

```py
def missing_upper_bound[**P]() -> None:
    constraints = ConstraintSet.lower_bound(Callable[[int], int], P)
    expected = ConstraintSet.range(Callable[[int], int], P, Top[Callable[..., object]])
    static_assert(constraints == expected)
```

### Invalid forms and preservation controls

An ordinary type is not a parameter list, so it makes a ParamSpec constraint unsatisfiable.

```py
from typing import Callable, Never, TypeVarTuple
from typing_extensions import TypeForm
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def invalid_bounds[**P]() -> None:
    static_assert(ConstraintSet.range(int, P, Callable[[int], None]) == ConstraintSet.never())
    static_assert(ConstraintSet.range(Callable[[int], None], P, int) == ConstraintSet.never())
    static_assert(ConstraintSet.lower_bound(int, P) == ConstraintSet.never())
    static_assert(ConstraintSet.upper_bound(P, object) == ConstraintSet.never())
    static_assert(ConstraintSet.equality(P, Never) == ConstraintSet.never())
```

An ordinary TypeVar or ParamSpec component is not a complete parameter list.

```py
def invalid_typevar_bounds[**P, **Q, T]() -> None:
    static_assert(ConstraintSet.range(T, P, Callable[[int], None]) == ConstraintSet.never())
    static_assert(ConstraintSet.range(Callable[[int], None], P, Q.args) == ConstraintSet.never())
    static_assert(ConstraintSet.range(Q.kwargs, P, Callable[[int], None]) == ConstraintSet.never())
```

Bare TypeVarTuples remain invalid bounds.

```py
def typevartuple_bounds[**P, *Us]() -> None:
    ConstraintSet.range(Us, P, Callable[[int], None])  # error: [invalid-type-form] "TypeVarTuple `Us`"
    ConstraintSet.range(Callable[[int], None], P, Us)  # error: [invalid-type-form] "TypeVarTuple `Us`"
```

Nested callable annotations and unrelated TypeForm calls retain normal ParamSpec validation.

```py
def accepts_type_form(form: TypeForm[object]) -> TypeForm[object]:
    return form

def invalid_forms[**P]() -> None:
    ConstraintSet.equality(P, Callable[[P], None])  # error: [invalid-type-form]
    ConstraintSet.equality(P, Callable[..., P])  # error: [invalid-type-form]
    ConstraintSet.equality(P, accepts_type_form(P))  # error: [invalid-type-form]
    ConstraintSet.equality(P, Callable[[int], None])
    accepts_type_form(P)  # error: [invalid-type-form]
```

ParamSpec components keep their ordinary bounds.

```py
def components[**P]() -> None:
    args = ConstraintSet.range(tuple[int], P.args, tuple[object, ...])
    kwargs = ConstraintSet.range(dict[str, object], P.kwargs, dict[str, object])
    static_assert(args != ConstraintSet.never())
    static_assert(kwargs != ConstraintSet.never())
```

Bare TypeVarTuples remain invalid subjects.

```py
Ts = TypeVarTuple("Ts")

def legacy_typevartuple_subject(value: tuple[*Ts]) -> None:
    ConstraintSet.range(Callable[[int], None], Ts, Callable[[int], None])  # error: [invalid-type-form]

def typevartuple_subject[*Us]() -> None:
    ConstraintSet.range(Callable[[int], None], Us, Callable[[int], None])  # error: [invalid-type-form]
```

## Displaying constraints

The `with_detailed_display` method can be used to print out the boolean formula that a constraint
set represents. However, this method is only intended for debugging purposes, and we reserve the
right to change the rendering at any time! We therefore do _not_ have a battery of mdtests printing
out all of the different kinds of constraints described above. Here we just test that the method
exists, and provides more detail than otherwise.

```py
from ty_extensions._internal import ConstraintSet, RegularCallableTypeOf

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

Explicit bottom and top parameter-list bounds are shown in the constraint.

```py
from typing import Any, Callable, Never
from ty_extensions import Bottom, Top

def explicit_bounds[**P]() -> None:
    lower = ConstraintSet.range(Bottom[Callable[..., Never]], P, Callable[[int], int])
    # revealed: ConstraintSet[((*args: object, **kwargs: object) ≤ P@explicit_bounds ≤ (int, /))]
    reveal_type(lower.with_detailed_display())
    upper = ConstraintSet.range(Callable[[int], int], P, Top[Callable[..., object]])
    # revealed: ConstraintSet[((int, /) ≤ P@explicit_bounds ≤ Top[(...)])]
    reveal_type(upper.with_detailed_display())
```

ParamSpec bounds display the full parameter list without the callable return type.

```py
def complete(value: int, /, text: str = "", *args: float, flag: bool = False, **kwargs: bytes) -> int:
    return 0

def signature[**P]() -> None:
    constraints = ConstraintSet.range(RegularCallableTypeOf[complete], P, RegularCallableTypeOf[complete])
    # revealed: ConstraintSet[(P@signature = (value: int, /, text: str = "", *args: float, flag: bool = False, **kwargs: bytes))]
    reveal_type(constraints.with_detailed_display())
```

Generic callable bounds keep their own ParamSpec binder.

```py
def callback[**Q](*args: Q.args, **kwargs: Q.kwargs) -> None: ...
def generic_signature[**P]() -> None:
    constraints = ConstraintSet.range(RegularCallableTypeOf[callback], P, RegularCallableTypeOf[callback])
    # revealed: ConstraintSet[(P@generic_signature = (**Q@callback))]
    reveal_type(constraints.with_detailed_display())
```

The display distinguishes gradual parameter lists from one required `Any` parameter.

```py
def gradual[**P]() -> None:
    ellipsis = ConstraintSet.range(Callable[..., int], P, Callable[..., str])
    reveal_type(ellipsis.with_detailed_display())  # revealed: ConstraintSet[(P@gradual = (...))]
    any_parameter = ConstraintSet.range(Callable[[Any], int], P, Callable[[Any], str])
    reveal_type(any_parameter.with_detailed_display())  # revealed: ConstraintSet[(P@gradual = (Any, /))]
```

Omitted bounds stay absent; explicit `...` bounds remain visible.

```py
def missing_bounds[**P]() -> None:
    # revealed: ConstraintSet[((int, /) ≤ P@missing_bounds)]
    reveal_type(ConstraintSet.lower_bound(Callable[[int], None], P).with_detailed_display())
    # revealed: ConstraintSet[((int, /) ≤ P@missing_bounds ≤ (...))]
    reveal_type(ConstraintSet.range(Callable[[int], None], P, Callable[..., None]).with_detailed_display())
    # revealed: ConstraintSet[(P@missing_bounds ≤ (int, /))]
    reveal_type(ConstraintSet.upper_bound(P, Callable[[int], None]).with_detailed_display())
    # revealed: ConstraintSet[((...) ≤ P@missing_bounds ≤ (int, /))]
    reveal_type(ConstraintSet.range(Callable[..., None], P, Callable[[int], None]).with_detailed_display())
```
