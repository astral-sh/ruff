# Constraint set satisfaction

```toml
[environment]
python-version = "3.12"
```

Constraint sets exist to help us check assignability and subtyping of types in the presence of
typevars. We construct a constraint set describing the conditions under which assignability holds
between the two types. Then we check whether that constraint set is satisfied for the valid
specializations of the relevant typevars. This file tests that final step.

## Inferable vs non-inferable typevars

Typevars can appear in _inferable_ or _non-inferable_ positions.

When a typevar is in an inferable position, the constraint set only needs to be satisfied for _some_
valid specialization. The most common inferable position occurs when invoking a generic function:
all of the function's typevars are inferable, because we want to use the argument types to infer
which specialization is being invoked.

When a typevar is in a non-inferable position, the constraint set must be satisfied for _every_
valid specialization. The most common non-inferable position occurs in the body of a generic
function or class: here we don't know in advance what type the typevar will be specialized to, and
so we have to ensure that the body is valid for all possible specializations.

```py
def f[T](t: T) -> T:
    # In the function body, T is non-inferable. All assignability checks involving T must be
    # satisfied for _all_ valid specializations of T.
    return t

# When invoking the function, T is inferable — we attempt to infer a specialization that is valid
# for the particular arguments that are passed to the function. Assignability checks (in particular,
# that the argument type is assignable to the parameter type) only need to succeed for _at least
# one_ specialization.
f(1)
```

In all of the examples below, for ease of reproducibility, we explicitly list the typevars that are
inferable in each `satisfied_by_all_typevars` call; any typevar not listed is assumed to be
non-inferable.

## Unbounded typevar

If a typevar has no bound or constraints, then it can specialize to any type. In an inferable
position, that means we just need a single type (any type at all!) that satisfies the constraint
set. In a non-inferable position, that means the constraint set must be satisfied for every possible
type.

```py
from typing import final, Never
from ty_extensions import ConstraintSet, static_assert

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def unbounded[T]():
    static_assert(ConstraintSet.always().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.always().satisfied_by_all_typevars())

    static_assert(not ConstraintSet.never().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(not ConstraintSet.never().satisfied_by_all_typevars())

    # (T = Never) is a valid specialization, which satisfies (T ≤ Unrelated).
    static_assert(ConstraintSet.range(Never, T, Unrelated).satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Base) is a valid specialization, which does not satisfy (T ≤ Unrelated).
    static_assert(not ConstraintSet.range(Never, T, Unrelated).satisfied_by_all_typevars())

    # (T = Base) is a valid specialization, which satisfies (T ≤ Super).
    static_assert(ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Unrelated) is a valid specialization, which does not satisfy (T ≤ Super).
    static_assert(not ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars())

    # (T = Base) is a valid specialization, which satisfies (T ≤ Base).
    static_assert(ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Unrelated) is a valid specialization, which does not satisfy (T ≤ Base).
    static_assert(not ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars())

    # (T = Sub) is a valid specialization, which satisfies (T ≤ Sub).
    static_assert(ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Unrelated) is a valid specialization, which does not satisfy (T ≤ Sub).
    static_assert(not ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars())
```

## Typevar with an upper bound

If a typevar has an upper bound, then it must specialize to a type that is a subtype of that bound.
For an inferable typevar, that means we need a single type that satisfies both the constraint set
and the upper bound. For a non-inferable typevar, that means the constraint set must be satisfied
for every type that satisfies the upper bound.

```py
from typing import final, Never
from ty_extensions import ConstraintSet, static_assert

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def bounded[T: Base]():
    static_assert(ConstraintSet.always().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.always().satisfied_by_all_typevars())

    static_assert(not ConstraintSet.never().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(not ConstraintSet.never().satisfied_by_all_typevars())

    # (T = Base) is a valid specialization, which satisfies (T ≤ Super).
    static_assert(ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars(inferable=tuple[T]))
    # Every valid specialization satisfies (T ≤ Base). Since (Base ≤ Super), every valid
    # specialization also satisfies (T ≤ Super).
    static_assert(ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars())

    # (T = Base) is a valid specialization, which satisfies (T ≤ Base).
    static_assert(ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars(inferable=tuple[T]))
    # Every valid specialization satisfies (T ≤ Base).
    static_assert(ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars())

    # (T = Sub) is a valid specialization, which satisfies (T ≤ Sub).
    static_assert(ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Base) is a valid specialization, which does not satisfy (T ≤ Sub).
    static_assert(not ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars())

    # (T = Never) is a valid specialization, which satisfies (T ≤ Unrelated).
    constraints = ConstraintSet.range(Never, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Base) is a valid specialization, which does not satisfy (T ≤ Unrelated).
    static_assert(not constraints.satisfied_by_all_typevars())

    # Never is the only type that satisfies both (T ≤ Base) and (T ≤ Unrelated). So there is no
    # valid specialization that satisfies (T ≤ Unrelated ∧ T ≠ Never).
    constraints = constraints & ~ConstraintSet.range(Never, T, Never)
    static_assert(not constraints.satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(not constraints.satisfied_by_all_typevars())
```

## Constrained typevar

If a typevar has constraints, then it must specialize to one of those specific types. (Not to a
subtype of one of those types!) For an inferable typevar, that means we need the constraint set to
be satisfied by any one of the constraints. For a non-inferable typevar, that means we need the
constraint set to be satisfied by all of those constraints.

```py
from typing import final, Never
from ty_extensions import ConstraintSet, static_assert

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def constrained[T: (Base, Unrelated)]():
    static_assert(ConstraintSet.always().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(ConstraintSet.always().satisfied_by_all_typevars())

    static_assert(not ConstraintSet.never().satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(not ConstraintSet.never().satisfied_by_all_typevars())

    # (T = Unrelated) is a valid specialization, which satisfies (T ≤ Unrelated).
    static_assert(ConstraintSet.range(Never, T, Unrelated).satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Base) is a valid specialization, which does not satisfy (T ≤ Unrelated).
    static_assert(not ConstraintSet.range(Never, T, Unrelated).satisfied_by_all_typevars())

    # (T = Base) is a valid specialization, which satisfies (T ≤ Super).
    static_assert(ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Unrelated) is a valid specialization, which does not satisfy (T ≤ Super).
    static_assert(not ConstraintSet.range(Never, T, Super).satisfied_by_all_typevars())

    # (T = Base) is a valid specialization, which satisfies (T ≤ Base).
    static_assert(ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Unrelated) is a valid specialization, which does not satisfy (T ≤ Base).
    static_assert(not ConstraintSet.range(Never, T, Base).satisfied_by_all_typevars())

    # Neither (T = Base) nor (T = Unrelated) satisfy (T ≤ Sub).
    static_assert(not ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(not ConstraintSet.range(Never, T, Sub).satisfied_by_all_typevars())

    # (T = Base) and (T = Unrelated) both satisfy (T ≤ Super ∨ T ≤ Unrelated).
    constraints = ConstraintSet.range(Never, T, Super) | ConstraintSet.range(Never, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(constraints.satisfied_by_all_typevars())

    # (T = Base) and (T = Unrelated) both satisfy (T ≤ Base ∨ T ≤ Unrelated).
    constraints = ConstraintSet.range(Never, T, Base) | ConstraintSet.range(Never, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(constraints.satisfied_by_all_typevars())

    # (T = Unrelated) is a valid specialization, which satisfies (T ≤ Sub ∨ T ≤ Unrelated).
    constraints = ConstraintSet.range(Never, T, Sub) | ConstraintSet.range(Never, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Base) is a valid specialization, which does not satisfy (T ≤ Sub ∨ T ≤ Unrelated).
    static_assert(not constraints.satisfied_by_all_typevars())

    # (T = Unrelated) is a valid specialization, which satisfies (T = Super ∨ T = Unrelated).
    constraints = ConstraintSet.range(Super, T, Super) | ConstraintSet.range(Unrelated, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Base) is a valid specialization, which does not satisfy (T = Super ∨ T = Unrelated).
    static_assert(not constraints.satisfied_by_all_typevars())

    # (T = Base) and (T = Unrelated) both satisfy (T = Base ∨ T = Unrelated).
    constraints = ConstraintSet.range(Base, T, Base) | ConstraintSet.range(Unrelated, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[T]))
    static_assert(constraints.satisfied_by_all_typevars())

    # (T = Unrelated) is a valid specialization, which satisfies (T = Sub ∨ T = Unrelated).
    constraints = ConstraintSet.range(Sub, T, Sub) | ConstraintSet.range(Unrelated, T, Unrelated)
    static_assert(constraints.satisfied_by_all_typevars(inferable=tuple[T]))
    # (T = Base) is a valid specialization, which does not satisfy (T = Sub ∨ T = Unrelated).
    static_assert(not constraints.satisfied_by_all_typevars())
```
