# Constraint set satisfaction

```toml
[environment]
python-version = "3.12"
```

Constraint sets exist to help us check assignability and subtyping of types in the presence of
typevars. We construct a constraint set describing the conditions under which assignability holds
between the two types. Then we check whether that constraint set is satisfied for the valid
specializations of the relevant typevars. This file tests that final step: ensuring that a
constraint set is satisfied given a list of inferable and non-inferable typevars.

(Note that in this file, for ease of reproducibility, we explicitly list the typevars that are
inferable in each `satisfied_by_all_typevars` call; any typevar not listed is assumed to be
non-inferable.)

## Inferable typevars

When a typevar is in an inferable position, the constraint set only needs to be satisfied for _some_
valid specialization. The most common inferable position occurs when invoking a generic function:
all of the function's typevars are inferable, because we want to use the argument types to infer
which specialization is being invoked.

```py
from typing import final, Never
from ty_extensions import ConstraintSet, static_assert

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...
```

If a typevar has no bound or constraints, then it can specialize to any type. For an inferable
typevar, that means we just need a single type (any type at all!) that satisfies the constraint set.

```py
def unbounded[T]():
    static_assert(ConstraintSet.always().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(not ConstraintSet.never().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.range(Never, T, Unrelated).satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars(inferable=tuple[T]))
```

If a typevar has an upper bound, then it must specialize to a type that is a subtype of that bound.
For an inferable typevar, that means we need a single type that satisfies both the constraint set
and the upper bound.

```py
def bounded[T: Base]():
    static_assert(ConstraintSet.always().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(not ConstraintSet.never().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars(inferable=tuple[T]))

    # This succeeds because T can specialize to Never
    constraints = ConstraintSet.range(Never, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[T]))

    # If we explicitly disallow Never, then it fails
    constraints = constraints & ~ConstraintSet.range(Never, T, Never)
    static_assert(not constraints.satisfied_by_all_typevars(inferable=tuple[T]))
```

If a typevar has constraints, then it must specialize to one of those specific types. (Not to a
subtype of one of those types!) For an inferable typevar, that means we need the constraint set to
be satisfied by any one of the constraints.

```py
def constrained[T: (Base, Unrelated)]():
    static_assert(ConstraintSet.always().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(not ConstraintSet.never().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.range(Never, T, Unrelated).satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars(inferable=tuple[T]))
    # Sub is not Base! Constraints must be exact, not subtypes
    static_assert(not ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars(inferable=tuple[T]))
```

## Non-inferable typevars

When a typevar is in a non-inferable position, the constraint set must be satisfied for _every_
valid specialization. The most common non-inferable position occurs in the body of a generic
function or class: here we don't know in advance what type the typevar will be specialized to, and
so we have to ensure that the body is valid for all possible specializations.

```py
from typing import final, Never
from ty_extensions import ConstraintSet, static_assert

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...
```

If a typevar has no bound or constraints, then it can specialize to any type. For a non-inferable
typevar, that means the constraint set must be satisfied for every possible type.

```py
def unbounded[T]():
    static_assert(ConstraintSet.always().satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.never().satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.range(Never, T, Unrelated).satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars(inferable=tuple[()]))
```

If a typevar has an upper bound, then it must specialize to a type that is a subtype of that bound.
For a non-inferable typevar, that means the constraint set must be satisfied for every type that
satisfies the upper bound.

```py
def bounded[T: Base]():
    static_assert(ConstraintSet.always().satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.never().satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.range(Never, T, Unrelated).satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars(inferable=tuple[()]))
```

If a typevar has constraints, then it must specialize to one of those specific types. (Not to a
subtype of one of those types!) For a non-inferable typevar, that means we need the constraint set
to be satisfied by all of those constraints.

```py
def constrained[T: (Base, Unrelated)]():
    static_assert(ConstraintSet.always().satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.never().satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.range(Never, T, Unrelated).satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars(inferable=tuple[()]))
    static_assert(not ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars(inferable=tuple[()]))

    constraints = ConstraintSet.range(Never, T, Super) | ConstraintSet.range(Never, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[()]))
    constraints = ConstraintSet.range(Never, T, Base) | ConstraintSet.range(Never, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[()]))
    constraints = ConstraintSet.range(Never, T, Sub) | ConstraintSet.range(Never, T, Unrelated)
    static_assert(not constraints.satisfied_by_all_typevars(inferable=tuple[()]))

    constraints = ConstraintSet.range(Super, T, Super) | ConstraintSet.range(Unrelated, T, Unrelated)
    static_assert(not constraints.satisfied_by_all_typevars(inferable=tuple[()]))
    constraints = ConstraintSet.range(Base, T, Base) | ConstraintSet.range(Unrelated, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[()]))
    constraints = ConstraintSet.range(Sub, T, Sub) | ConstraintSet.range(Unrelated, T, Unrelated)
    static_assert(not constraints.satisfied_by_all_typevars(inferable=tuple[()]))
```
