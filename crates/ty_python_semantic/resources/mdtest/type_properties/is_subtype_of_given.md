# Constraint implication

```toml
[environment]
python-version = "3.12"
```

This file tests the _constraint implication_ relationship between types, aka `is_subtype_of_given`,
which tests whether one type is a [subtype][subtyping] of another _assuming that the constraints in
a particular constraint set hold_.

## Concrete types

For concrete types, constraint implication is exactly the same as subtyping. (A concrete type is any
fully static type that is not a typevar. It can _contain_ a typevar, though — `list[T]` is
considered concrete.)

```py
from ty_extensions import is_subtype_of, is_subtype_of_given, static_assert

def equivalent_to_other_relationships[T]():
    static_assert(is_subtype_of(bool, int))
    static_assert(is_subtype_of_given(True, bool, int))

    static_assert(not is_subtype_of(bool, str))
    static_assert(not is_subtype_of_given(True, bool, str))
```

Moreover, for concrete types, the answer does not depend on which constraint set we are considering.
`bool` is a subtype of `int` no matter what types any typevars are specialized to — and even if
there isn't a valid specialization for the typevars we are considering.

```py
from typing import Never
from ty_extensions import range_constraint

def even_given_constraints[T]():
    constraints = range_constraint(Never, T, int)
    static_assert(is_subtype_of_given(constraints, bool, int))
    static_assert(not is_subtype_of_given(constraints, bool, str))

def even_given_unsatisfiable_constraints():
    static_assert(is_subtype_of_given(False, bool, int))
    static_assert(not is_subtype_of_given(False, bool, str))
```

## Type variables

The interesting case is typevars. The other typing relationships (TODO: will) all "punt" on the
question when considering a typevar, by translating the desired relationship into a constraint set.

```py
from typing import Any
from ty_extensions import is_assignable_to, is_subtype_of

def assignability[T]():
    # TODO: revealed: ty_extensions.ConstraintSet[T@assignability ≤ bool]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_assignable_to(T, bool))
    # TODO: revealed: ty_extensions.ConstraintSet[T@assignability ≤ int]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_assignable_to(T, int))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(is_assignable_to(T, object))

def subtyping[T]():
    # TODO: revealed: ty_extensions.ConstraintSet[T@subtyping ≤ bool]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_subtype_of(T, bool))
    # TODO: revealed: ty_extensions.ConstraintSet[T@subtyping ≤ int]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_subtype_of(T, int))
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(is_subtype_of(T, object))
```

When checking assignability with a dynamic type, we use the bottom and top materializations of the
lower and upper bounds, respectively. For subtyping, we use the top and bottom materializations.
(That is, assignability turns into a "permissive" constraint, and subtyping turns into a
"conservative" constraint.)

```py
class Covariant[T]:
    def get(self) -> T:
        raise ValueError

class Contravariant[T]:
    def set(self, value: T):
        pass

def assignability[T]():
    # aka [T@assignability ≤ object], which is always satisfiable
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(is_assignable_to(T, Any))

    # aka [Never ≤ T@assignability], which is always satisfiable
    # revealed: ty_extensions.ConstraintSet[always]
    reveal_type(is_assignable_to(Any, T))

    # TODO: revealed: ty_extensions.ConstraintSet[T@assignability ≤ Covariant[object]]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_assignable_to(T, Covariant[Any]))
    # TODO: revealed: ty_extensions.ConstraintSet[Covariant[Never] ≤ T@assignability]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_assignable_to(Covariant[Any], T))

    # TODO: revealed: ty_extensions.ConstraintSet[T@assignability ≤ Contravariant[Never]]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_assignable_to(T, Contravariant[Any]))
    # TODO: revealed: ty_extensions.ConstraintSet[Contravariant[object] ≤ T@assignability]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_assignable_to(Contravariant[Any], T))

def subtyping[T]():
    # aka [T@assignability ≤ object], which is always satisfiable
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_subtype_of(T, Any))

    # aka [Never ≤ T@assignability], which is always satisfiable
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_subtype_of(Any, T))

    # TODO: revealed: ty_extensions.ConstraintSet[T@subtyping ≤ Covariant[Never]]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_subtype_of(T, Covariant[Any]))
    # TODO: revealed: ty_extensions.ConstraintSet[Covariant[object] ≤ T@subtyping]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_subtype_of(Covariant[Any], T))

    # TODO: revealed: ty_extensions.ConstraintSet[T@subtyping ≤ Contravariant[object]]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_subtype_of(T, Contravariant[Any]))
    # TODO: revealed: ty_extensions.ConstraintSet[Contravariant[Never] ≤ T@subtyping]
    # revealed: ty_extensions.ConstraintSet[never]
    reveal_type(is_subtype_of(Contravariant[Any], T))
```

At some point, though, we need to resolve a constraint set; at that point, we can no longer punt on
the question. Unlike with concrete types, the answer will depend on the constraint set that we are
considering.

```py
from typing import Never
from ty_extensions import is_subtype_of_given, range_constraint, static_assert

def given_constraints[T]():
    static_assert(not is_subtype_of_given(True, T, int))
    static_assert(not is_subtype_of_given(True, T, bool))
    static_assert(not is_subtype_of_given(True, T, str))

    # These are vacuously true; false implies anything
    static_assert(is_subtype_of_given(False, T, int))
    static_assert(is_subtype_of_given(False, T, bool))
    static_assert(is_subtype_of_given(False, T, str))

    given_int = range_constraint(Never, T, int)
    static_assert(is_subtype_of_given(given_int, T, int))
    static_assert(not is_subtype_of_given(given_int, T, bool))
    static_assert(not is_subtype_of_given(given_int, T, str))

    given_bool = range_constraint(Never, T, bool)
    static_assert(is_subtype_of_given(given_bool, T, int))
    static_assert(is_subtype_of_given(given_bool, T, bool))
    static_assert(not is_subtype_of_given(given_bool, T, str))

    given_both = given_bool & given_int
    static_assert(is_subtype_of_given(given_both, T, int))
    static_assert(is_subtype_of_given(given_both, T, bool))
    static_assert(not is_subtype_of_given(given_both, T, str))

    given_str = range_constraint(Never, T, str)
    static_assert(not is_subtype_of_given(given_str, T, int))
    static_assert(not is_subtype_of_given(given_str, T, bool))
    static_assert(is_subtype_of_given(given_str, T, str))
```

This might require propagating constraints from other typevars.

```py
def mutually_constrained[T, U]():
    # If [T = U ∧ U ≤ int], then [T ≤ int] must be true as well.
    given_int = range_constraint(U, T, U) & range_constraint(Never, U, int)
    # TODO: no static-assert-error
    # error: [static-assert-error]
    static_assert(is_subtype_of_given(given_int, T, int))
    static_assert(not is_subtype_of_given(given_int, T, bool))
    static_assert(not is_subtype_of_given(given_int, T, str))

    # If [T ≤ U ∧ U ≤ int], then [T ≤ int] must be true as well.
    given_int = range_constraint(Never, T, U) & range_constraint(Never, U, int)
    # TODO: no static-assert-error
    # error: [static-assert-error]
    static_assert(is_subtype_of_given(given_int, T, int))
    static_assert(not is_subtype_of_given(given_int, T, bool))
    static_assert(not is_subtype_of_given(given_int, T, str))
```

[subtyping]: https://typing.python.org/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
