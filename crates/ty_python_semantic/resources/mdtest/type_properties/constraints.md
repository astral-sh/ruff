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
constraints_. Note that the constraint set that contains no clauses is never satisfiable (`⋃ {} =
0`); and the constraint set that contains a single clause, where that clause contains no
constraints, is always satisfiable (`⋃ {⋂ {}} = 1`).

## Kinds of constraints

### Range

A _range_ constraint requires the typevar to be within a particular lower and upper bound: the
typevar can only specialize to a type that is a supertype of the lower bound, and a subtype of the
upper bound.

```py
from typing import Any, Never, Sequence
from ty_extensions import range_constraint

class Super: ...
class Base(Super): ...
class Sub(Base): ...

def _[T]():
    # revealed: ty_extensions.ConstraintSet[(Sub ≤ T@_ ≤ Super)]
    reveal_type(range_constraint(Sub, T, Super))
```

Every type is a supertype of `Never`, so a lower bound of `Never` is the same as having no lower
bound.

```py
def _[T]():
    # revealed: ty_extensions.ConstraintSet[(T@_ ≤ Base)]
    reveal_type(range_constraint(Never, T, Base))
```

Similarly, every type is a subtype of `object`, so an upper bound of `object` is the same as having
no upper bound.

```py
def _[T]():
    # revealed: ty_extensions.ConstraintSet[(Base ≤ T@_)]
    reveal_type(range_constraint(Base, T, object))
```

And a range constraint with _both_ a lower bound of `Never` and an upper bound of `object` does not
constrain the typevar at all.

```py
def _[T]():
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(range_constraint(Never, T, object))
```

If the lower bound and upper bounds are "inverted" (the upper bound is a subtype of the lower
bound), then there is no type that can satisfy the constraint.

```py
def _[T]():
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(range_constraint(Super, T, Sub))
```

Constraints can only refer to fully static types, so the lower and upper bounds are transformed into
their bottom and top materializations, respectively.

```py
def _[T]():
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

def _[T]():
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Base)]
    reveal_type(not_equivalent_constraint(T, Base))
```

Unlike range constraints, `Never` and `object` are not special when used as the hole of a
not-equivalent constraint — there are many types that are not equivalent to `Never` or `object`.

```py
def _[T]():
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Never)]
    reveal_type(not_equivalent_constraint(T, Never))

    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ object)]
    reveal_type(not_equivalent_constraint(T, object))
```

Constraints can only refer to fully static types, so the hole is transformed into its top
materialization.

```py
def _[T]():
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ object)]
    reveal_type(not_equivalent_constraint(T, Any))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≠ Sequence[object])]
    reveal_type(not_equivalent_constraint(T, Sequence[Any]))
```

### Incomparable

An _incomparable_ constraint requires the typevar to specialize to any type that is neither a
subtype nor a supertype of a particular type (the "pivot").

```py
from typing import Any, Never, Sequence
from ty_extensions import incomparable_constraint

class Base: ...

def _[T]():
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Base)]
    reveal_type(incomparable_constraint(T, Base))
```

Every type is comparable with `Never` and with `object`, so an incomparable constraint with either
as a pivot cannot ever be satisfied.

```py
def _[T]():
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(incomparable_constraint(T, Never))

    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(incomparable_constraint(T, object))
```

Constraints can only refer to fully static types, so the pivot is transformed into its top
materialization.

```py
def _[T]():
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(incomparable_constraint(T, Any))
    # revealed: ty_extensions.ConstraintSet[(T@_ ≁ Sequence[object])]
    reveal_type(incomparable_constraint(T, Sequence[Any]))
```
