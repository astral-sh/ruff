# Constraint set quantification

```toml
[environment]
python-version = "3.12"
```

We can _existentially quantify_ a constraint set over a type variable. The result is a copy of the
constraint set that only mentions the requested typevar. All constraints mentioning any other
typevars are removed. Importantly, they are removed "safely", with their constraints propagated
through to the remaining constraints as needed.

## Keeping a single typevar

If a constraint set only mentions a single typevar, and we keep that typevar when quantifying, the
result is unchanged.

```py
from ty_extensions import ConstraintSet, static_assert

class Base: ...
class Sub(Base): ...

def keep_single[T]():
    constraints = ConstraintSet.always()
    quantified = ConstraintSet.always()
    static_assert(constraints.retain_one(T) == quantified)

    constraints = ConstraintSet.never()
    quantified = ConstraintSet.never()
    static_assert(constraints.retain_one(T) == quantified)

    constraints = ConstraintSet.range(Sub, T, Base)
    quantified = ConstraintSet.range(Sub, T, Base)
    static_assert(constraints.retain_one(T) == quantified)
```

## Removing a single typevar

If a constraint set only mentions a single typevar, and we remove that typevar when quantifying,
the result is usually "always". The only exception is if the original constraint set has no
solution. In that case, the result is also unsatisfiable.

```py
from ty_extensions import ConstraintSet, static_assert

class Base: ...
class Sub(Base): ...

def remove_single[T]():
    constraints = ConstraintSet.always()
    quantified = ConstraintSet.always()
    static_assert(constraints.exists(T) == quantified)

    constraints = ConstraintSet.never()
    quantified = ConstraintSet.never()
    static_assert(constraints.exists(T) == quantified)

    constraints = ConstraintSet.range(Sub, T, Base)
    quantified = ConstraintSet.always()
    static_assert(constraints.exists(T) == quantified)
```

This also holds when the constraint set contains multiple typevars. In the cases below, we are
keeping `U`, and the constraints on `T` do not ever affect what `U` can specialize to — `U` can
specialize to anything (unless the original constraint set is unsatisfiable).

```py
from ty_extensions import ConstraintSet, static_assert

class Base: ...
class Sub(Base): ...

def remove_other[T, U]():
    constraints = ConstraintSet.always()
    quantified = ConstraintSet.always()
    static_assert(constraints.retain_one(U) == quantified)

    constraints = ConstraintSet.never()
    quantified = ConstraintSet.never()
    static_assert(constraints.retain_one(U) == quantified)

    constraints = ConstraintSet.range(Sub, T, Base)
    quantified = ConstraintSet.always()
    static_assert(constraints.retain_one(U) == quantified)
```

## Transitivity

When a constraint set mentions two typevars, and compares them directly, then we can use
transitivity to propagate the other constraints when quantifying.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Super: ...
class Base(Super): ...
class Sub(Base): ...

def transitivity[T, U]():
    # (Base ≤ T) ∧ (T ≤ U) → (Base ≤ U)
    constraints = ConstraintSet.range(Base, T, object) & ConstraintSet.range(T, U, object)
    quantified = ConstraintSet.range(Base, U, object)
    static_assert(constraints.exists(T) == quantified)

    # (Base ≤ T ≤ Super) ∧ (T ≤ U) → (Base ≤ U)
    constraints = ConstraintSet.range(Base, T, Super) & ConstraintSet.range(T, U, object)
    quantified = ConstraintSet.range(Base, U, object)
    static_assert(constraints.exists(T) == quantified)

    # (T ≤ Base) ∧ (U ≤ T) → (U ≤ Base)
    constraints = ConstraintSet.range(Never, T, Base) & ConstraintSet.range(Never, U, T)
    quantified = ConstraintSet.range(Never, U, Base)
    static_assert(constraints.exists(T) == quantified)

    # (Sub ≤ T ≤ Base) ∧ (U ≤ T) → (U ≤ Base)
    constraints = ConstraintSet.range(Sub, T, Base) & ConstraintSet.range(Never, U, T)
    quantified = ConstraintSet.range(Never, U, Base)
    static_assert(constraints.exists(T) == quantified)
```

## Covariant transitivity

The same applies when one of the typevars is used covariantly in a bound of the other typevar.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Super: ...
class Base(Super): ...
class Sub(Base): ...

class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

def covariant_transitivity[T, U]():
    # (Base ≤ T) ∧ (Covariant[T] ≤ U) → (Covariant[Base] ≤ U)
    constraints = ConstraintSet.range(Base, T, object) & ConstraintSet.range(Covariant[T], U, object)
    quantified = ConstraintSet.range(Covariant[Base], U, object)
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)

    # (Base ≤ T ≤ Super) ∧ (Covariant[T] ≤ U) → (Covariant[Base] ≤ U)
    constraints = ConstraintSet.range(Base, T, Super) & ConstraintSet.range(Covariant[T], U, object)
    quantified = ConstraintSet.range(Covariant[Base], U, object)
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)

    # (T ≤ Base) ∧ (U ≤ Covariant[T]) → (U ≤ Covariant[Base])
    constraints = ConstraintSet.range(Never, T, Base) & ConstraintSet.range(Never, U, Covariant[T])
    quantified = ConstraintSet.range(Never, U, Covariant[Base])
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)

    # (Sub ≤ T ≤ Base) ∧ (U ≤ Covariant[T]) → (U ≤ Covariant[Base])
    constraints = ConstraintSet.range(Sub, T, Base) & ConstraintSet.range(Never, U, Covariant[T])
    quantified = ConstraintSet.range(Never, U, Covariant[Base])
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)
```

## Contravariant transitivity

Similar rules apply, but in reverse, when one of the typevars is used contravariantly in a bound of
the other typevar.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Super: ...
class Base(Super): ...
class Sub(Base): ...

class Contravariant[T]:
    def receive(self, input: T): ...

def contravariant_transitivity[T, U]():
    # (Base ≤ T) ∧ (U ≤ Contravariant[T]) → (U ≤ Contravariant[Base])
    constraints = ConstraintSet.range(Base, T, object) & ConstraintSet.range(Never, U, Contravariant[T])
    quantified = ConstraintSet.range(Never, U, Contravariant[Base])
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)

    # (Base ≤ T ≤ Super) ∧ (U ≤ Contravariant[T]) → (U ≤ Contravariant[Base])
    constraints = ConstraintSet.range(Base, T, Super) & ConstraintSet.range(Never, U, Contravariant[T])
    quantified = ConstraintSet.range(Never, U, Contravariant[Base])
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)

    # (T ≤ Base) ∧ (Contravariant[T] ≤ U) → (Contravariant[Base] ≤ U)
    constraints = ConstraintSet.range(Never, T, Base) & ConstraintSet.range(Contravariant[T], U, object)
    quantified = ConstraintSet.range(Contravariant[Base], U, object)
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)

    # (Sub ≤ T ≤ Base) ∧ (Contravariant[T] ≤ U) → (Contravariant[Base] ≤ U)
    constraints = ConstraintSet.range(Sub, T, Base) & ConstraintSet.range(Contravariant[T], U, object)
    quantified = ConstraintSet.range(Contravariant[Base], U, object)
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)
```

## Invariant transitivity involving equality constraints

Invariant uses of a typevar are more subtle. The simplest case is when there is an _equality_
constraint on the invariant typevar. In that case, we know precisely which specialization is
required.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Base: ...

class Invariant[T]:
    mutable_attribute: T

def invariant_equality_transitivity[T, U]():
    # (T = Base) ∧ (U ≤ Invariant[T]) → (U ≤ Invariant[Base])
    constraints = ConstraintSet.range(Base, T, Base) & ConstraintSet.range(Never, U, Invariant[T])
    quantified = ConstraintSet.range(Never, U, Invariant[Base])
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)

    # (T = Base) ∧ (Invariant[T] ≤ U) → (Invariant[Base] ≤ U)
    constraints = ConstraintSet.range(Base, T, Base) & ConstraintSet.range(Invariant[T], U, object)
    quantified = ConstraintSet.range(Invariant[Base], U, object)
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)
```

## Invariant transitivity involving range constraints

When there is a _range_ constraint on the invariant typevar, we still have to retain information
about which range of types the quantified-away typevar can specialize to, since this affects which
types the remaining typevar can specialize to, and invariant typevars are not monotonic like
covariant and contravariant typevars.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Base: ...
class Sub(Base): ...

class Invariant[T]:
    mutable_attribute: T

def invariant_range_transitivity[T, U]():
    # (Sub ≤ T ≤ Base) ∧ (U ≤ Invariant[T]) → (U ≤ Invariant[Exists[Sub, Base]])
    constraints = ConstraintSet.range(Sub, T, Base) & ConstraintSet.range(Never, U, Invariant[T])
    # TODO: The existential that we need doesn't exist yet.
    quantified = ConstraintSet.never()
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)

    # (Sub ≤ T ≤ Base) ∧ (Invariant[T] ≤ U) → (Invariant[Exists[Sub, Base]] ≤ U)
    constraints = ConstraintSet.range(Sub, T, Base) & ConstraintSet.range(Invariant[T], U, object)
    # TODO: The existential that we need doesn't exist yet.
    quantified = ConstraintSet.never()
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.exists(T) == quantified)
```
