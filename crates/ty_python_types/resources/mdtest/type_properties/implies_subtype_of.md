# Constraint implication

```toml
[environment]
python-version = "3.12"
```

This file tests the _constraint implication_ relationship between types, aka `implies_subtype_of`,
which tests whether one type is a [subtype][subtyping] of another _assuming that the constraints in
a particular constraint set hold_.

## Concrete types

For concrete types, constraint implication is exactly the same as subtyping. (A concrete type is any
fully static type that does not contain a typevar.)

```py
from ty_extensions import ConstraintSet, is_subtype_of, static_assert

def equivalent_to_other_relationships[T]():
    static_assert(is_subtype_of(bool, int))
    static_assert(ConstraintSet.always().implies_subtype_of(bool, int))

    static_assert(not is_subtype_of(bool, str))
    static_assert(not ConstraintSet.always().implies_subtype_of(bool, str))
```

Moreover, for concrete types, the answer does not depend on which constraint set we are considering.
`bool` is a subtype of `int` no matter what types any typevars are specialized to — and even if
there isn't a valid specialization for the typevars we are considering.

```py
from typing import Never
from ty_extensions import ConstraintSet

def even_given_constraints[T]():
    constraints = ConstraintSet.range(Never, T, int)
    static_assert(constraints.implies_subtype_of(bool, int))
    static_assert(not constraints.implies_subtype_of(bool, str))

def even_given_unsatisfiable_constraints():
    static_assert(ConstraintSet.never().implies_subtype_of(bool, int))
    static_assert(not ConstraintSet.never().implies_subtype_of(bool, str))
```

## Type variables

The interesting case is typevars. The other typing relationships (TODO: will) all "punt" on the
question when considering a typevar, by translating the desired relationship into a constraint set.

```py
from typing import Any
from ty_extensions import ConstraintSet, is_assignable_to, is_subtype_of, static_assert

def assignability[T]():
    constraints = is_assignable_to(T, bool)
    # TODO: expected = ConstraintSet.range(Never, T, bool)
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_assignable_to(T, int)
    # TODO: expected = ConstraintSet.range(Never, T, int)
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_assignable_to(T, object)
    expected = ConstraintSet.always()
    static_assert(constraints == expected)

def subtyping[T]():
    constraints = is_subtype_of(T, bool)
    # TODO: expected = ConstraintSet.range(Never, T, bool)
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_subtype_of(T, int)
    # TODO: expected = ConstraintSet.range(Never, T, int)
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_subtype_of(T, object)
    expected = ConstraintSet.always()
    static_assert(constraints == expected)
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
    constraints = is_assignable_to(T, Any)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = is_assignable_to(Any, T)
    expected = ConstraintSet.range(Never, T, object)
    static_assert(constraints == expected)

    constraints = is_assignable_to(T, Covariant[Any])
    # TODO: expected = ConstraintSet.range(Never, T, Covariant[object])
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_assignable_to(Covariant[Any], T)
    # TODO: expected = ConstraintSet.range(Covariant[Never], T, object)
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_assignable_to(T, Contravariant[Any])
    # TODO: expected = ConstraintSet.range(Never, T, Contravariant[Never])
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_assignable_to(Contravariant[Any], T)
    # TODO: expected = ConstraintSet.range(Contravariant[object], T, object)
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

def subtyping[T]():
    constraints = is_subtype_of(T, Any)
    # TODO: expected = ConstraintSet.range(Never, T, Never)
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_subtype_of(Any, T)
    # TODO: expected = ConstraintSet.range(object, T, object)
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_subtype_of(T, Covariant[Any])
    # TODO: expected = ConstraintSet.range(Never, T, Covariant[Never])
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_subtype_of(Covariant[Any], T)
    # TODO: expected = ConstraintSet.range(Covariant[object], T, object)
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_subtype_of(T, Contravariant[Any])
    # TODO: expected = ConstraintSet.range(Never, T, Contravariant[object])
    expected = ConstraintSet.never()
    static_assert(constraints == expected)

    constraints = is_subtype_of(Contravariant[Any], T)
    # TODO: expected = ConstraintSet.range(Contravariant[Never], T, object)
    expected = ConstraintSet.never()
    static_assert(constraints == expected)
```

At some point, though, we need to resolve a constraint set; at that point, we can no longer punt on
the question. Unlike with concrete types, the answer will depend on the constraint set that we are
considering.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

def given_constraints[T]():
    static_assert(not ConstraintSet.always().implies_subtype_of(T, int))
    static_assert(not ConstraintSet.always().implies_subtype_of(T, bool))
    static_assert(not ConstraintSet.always().implies_subtype_of(T, str))

    # These are vacuously true; false implies anything
    static_assert(ConstraintSet.never().implies_subtype_of(T, int))
    static_assert(ConstraintSet.never().implies_subtype_of(T, bool))
    static_assert(ConstraintSet.never().implies_subtype_of(T, str))

    given_int = ConstraintSet.range(Never, T, int)
    static_assert(given_int.implies_subtype_of(T, int))
    static_assert(not given_int.implies_subtype_of(T, bool))
    static_assert(not given_int.implies_subtype_of(T, str))

    given_bool = ConstraintSet.range(Never, T, bool)
    static_assert(given_bool.implies_subtype_of(T, int))
    static_assert(given_bool.implies_subtype_of(T, bool))
    static_assert(not given_bool.implies_subtype_of(T, str))

    given_both = given_bool & given_int
    static_assert(given_both.implies_subtype_of(T, int))
    static_assert(given_both.implies_subtype_of(T, bool))
    static_assert(not given_both.implies_subtype_of(T, str))

    given_str = ConstraintSet.range(Never, T, str)
    static_assert(not given_str.implies_subtype_of(T, int))
    static_assert(not given_str.implies_subtype_of(T, bool))
    static_assert(given_str.implies_subtype_of(T, str))
```

This might require propagating constraints from other typevars. (Note that we perform the test
twice, with different variable orderings. Our BDD implementation uses the Salsa IDs of each typevar
as part of the variable ordering. Reversing the typevar order helps us verify that we don't have any
BDD logic that is dependent on which variable ordering we end up with.)

```py
def mutually_constrained[T, U]():
    # If [T = U ∧ U ≤ int], then [T ≤ int] must be true as well.
    given_int = ConstraintSet.range(U, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(T, int))
    static_assert(not given_int.implies_subtype_of(T, bool))
    static_assert(not given_int.implies_subtype_of(T, str))

    # If [T ≤ U ∧ U ≤ int], then [T ≤ int] must be true as well.
    given_int = ConstraintSet.range(Never, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(T, int))
    static_assert(not given_int.implies_subtype_of(T, bool))
    static_assert(not given_int.implies_subtype_of(T, str))

def mutually_constrained[U, T]():
    # If [T = U ∧ U ≤ int], then [T ≤ int] must be true as well.
    given_int = ConstraintSet.range(U, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(T, int))
    static_assert(not given_int.implies_subtype_of(T, bool))
    static_assert(not given_int.implies_subtype_of(T, str))

    # If [T ≤ U ∧ U ≤ int], then [T ≤ int] must be true as well.
    given_int = ConstraintSet.range(Never, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(T, int))
    static_assert(not given_int.implies_subtype_of(T, bool))
    static_assert(not given_int.implies_subtype_of(T, str))
```

## Compound types

All of the relationships in the above section also apply when a typevar appears in a compound type.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Covariant[T]:
    def get(self) -> T:
        raise ValueError

def given_constraints[T]():
    static_assert(not ConstraintSet.always().implies_subtype_of(Covariant[T], Covariant[int]))
    static_assert(not ConstraintSet.always().implies_subtype_of(Covariant[T], Covariant[bool]))
    static_assert(not ConstraintSet.always().implies_subtype_of(Covariant[T], Covariant[str]))

    # These are vacuously true; false implies anything
    static_assert(ConstraintSet.never().implies_subtype_of(Covariant[T], Covariant[int]))
    static_assert(ConstraintSet.never().implies_subtype_of(Covariant[T], Covariant[bool]))
    static_assert(ConstraintSet.never().implies_subtype_of(Covariant[T], Covariant[str]))

    # For a covariant typevar, (T ≤ int) implies that (Covariant[T] ≤ Covariant[int]).
    given_int = ConstraintSet.range(Never, T, int)
    static_assert(given_int.implies_subtype_of(Covariant[T], Covariant[int]))
    static_assert(not given_int.implies_subtype_of(Covariant[T], Covariant[bool]))
    static_assert(not given_int.implies_subtype_of(Covariant[T], Covariant[str]))

    given_bool = ConstraintSet.range(Never, T, bool)
    static_assert(given_bool.implies_subtype_of(Covariant[T], Covariant[int]))
    static_assert(given_bool.implies_subtype_of(Covariant[T], Covariant[bool]))
    static_assert(not given_bool.implies_subtype_of(Covariant[T], Covariant[str]))

    given_bool_int = ConstraintSet.range(bool, T, int)
    static_assert(not given_bool_int.implies_subtype_of(Covariant[int], Covariant[T]))
    static_assert(given_bool_int.implies_subtype_of(Covariant[bool], Covariant[T]))
    static_assert(not given_bool_int.implies_subtype_of(Covariant[str], Covariant[T]))

def mutually_constrained[T, U]():
    # If (T = U ∧ U ≤ int), then (T ≤ int) must be true as well, and therefore
    # (Covariant[T] ≤ Covariant[int]).
    given_int = ConstraintSet.range(U, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(Covariant[T], Covariant[int]))
    static_assert(not given_int.implies_subtype_of(Covariant[T], Covariant[bool]))
    static_assert(not given_int.implies_subtype_of(Covariant[T], Covariant[str]))

    # If (T ≤ U ∧ U ≤ int), then (T ≤ int) must be true as well, and therefore
    # (Covariant[T] ≤ Covariant[int]).
    given_int = ConstraintSet.range(Never, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(Covariant[T], Covariant[int]))
    static_assert(not given_int.implies_subtype_of(Covariant[T], Covariant[bool]))
    static_assert(not given_int.implies_subtype_of(Covariant[T], Covariant[str]))

# Repeat the test with a different typevar ordering
def mutually_constrained[U, T]():
    # If (T = U ∧ U ≤ int), then (T ≤ int) must be true as well, and therefore
    # (Covariant[T] ≤ Covariant[int]).
    given_int = ConstraintSet.range(U, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(Covariant[T], Covariant[int]))
    static_assert(not given_int.implies_subtype_of(Covariant[T], Covariant[bool]))
    static_assert(not given_int.implies_subtype_of(Covariant[T], Covariant[str]))

    # If (T ≤ U ∧ U ≤ int), then (T ≤ int) must be true as well, and therefore
    # (Covariant[T] ≤ Covariant[int]).
    given_int = ConstraintSet.range(Never, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(Covariant[T], Covariant[int]))
    static_assert(not given_int.implies_subtype_of(Covariant[T], Covariant[bool]))
    static_assert(not given_int.implies_subtype_of(Covariant[T], Covariant[str]))
```

Many of the relationships are reversed for typevars that appear in contravariant types.

```py
class Contravariant[T]:
    def set(self, value: T):
        pass

def given_constraints[T]():
    static_assert(not ConstraintSet.always().implies_subtype_of(Contravariant[int], Contravariant[T]))
    static_assert(not ConstraintSet.always().implies_subtype_of(Contravariant[bool], Contravariant[T]))
    static_assert(not ConstraintSet.always().implies_subtype_of(Contravariant[str], Contravariant[T]))

    # These are vacuously true; false implies anything
    static_assert(ConstraintSet.never().implies_subtype_of(Contravariant[int], Contravariant[T]))
    static_assert(ConstraintSet.never().implies_subtype_of(Contravariant[bool], Contravariant[T]))
    static_assert(ConstraintSet.never().implies_subtype_of(Contravariant[str], Contravariant[T]))

    # For a contravariant typevar, (T ≤ int) implies that (Contravariant[int] ≤ Contravariant[T]).
    # (The order of the comparison is reversed because of contravariance.)
    given_int = ConstraintSet.range(Never, T, int)
    static_assert(given_int.implies_subtype_of(Contravariant[int], Contravariant[T]))
    static_assert(not given_int.implies_subtype_of(Contravariant[bool], Contravariant[T]))
    static_assert(not given_int.implies_subtype_of(Contravariant[str], Contravariant[T]))

    given_bool = ConstraintSet.range(Never, T, int)
    static_assert(given_bool.implies_subtype_of(Contravariant[int], Contravariant[T]))
    static_assert(not given_bool.implies_subtype_of(Contravariant[bool], Contravariant[T]))
    static_assert(not given_bool.implies_subtype_of(Contravariant[str], Contravariant[T]))

def mutually_constrained[T, U]():
    # If (T = U ∧ U ≤ int), then (T ≤ int) must be true as well, and therefore
    # (Contravariant[int] ≤ Contravariant[T]).
    given_int = ConstraintSet.range(U, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(Contravariant[int], Contravariant[T]))
    static_assert(not given_int.implies_subtype_of(Contravariant[bool], Contravariant[T]))
    static_assert(not given_int.implies_subtype_of(Contravariant[str], Contravariant[T]))

    # If (T ≤ U ∧ U ≤ int), then (T ≤ int) must be true as well, and therefore
    # (Contravariant[int] ≤ Contravariant[T]).
    given_int = ConstraintSet.range(Never, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(Contravariant[int], Contravariant[T]))
    static_assert(not given_int.implies_subtype_of(Contravariant[bool], Contravariant[T]))
    static_assert(not given_int.implies_subtype_of(Contravariant[str], Contravariant[T]))

# Repeat the test with a different typevar ordering
def mutually_constrained[U, T]():
    # If (T = U ∧ U ≤ int), then (T ≤ int) must be true as well, and therefore
    # (Contravariant[int] ≤ Contravariant[T]).
    given_int = ConstraintSet.range(U, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(Contravariant[int], Contravariant[T]))
    static_assert(not given_int.implies_subtype_of(Contravariant[bool], Contravariant[T]))
    static_assert(not given_int.implies_subtype_of(Contravariant[str], Contravariant[T]))

    # If (T ≤ U ∧ U ≤ int), then (T ≤ int) must be true as well, and therefore
    # (Contravariant[int] ≤ Contravariant[T]).
    given_int = ConstraintSet.range(Never, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(given_int.implies_subtype_of(Contravariant[int], Contravariant[T]))
    static_assert(not given_int.implies_subtype_of(Contravariant[bool], Contravariant[T]))
    static_assert(not given_int.implies_subtype_of(Contravariant[str], Contravariant[T]))
```

For invariant typevars, subtyping of the typevar does not imply subtyping of the compound type in
either direction. But an equality constraint on the typevar does.

```py
class Invariant[T]:
    def get(self) -> T:
        raise ValueError

    def set(self, value: T):
        pass

def given_constraints[T]():
    static_assert(not ConstraintSet.always().implies_subtype_of(Invariant[T], Invariant[int]))
    static_assert(not ConstraintSet.always().implies_subtype_of(Invariant[T], Invariant[bool]))
    static_assert(not ConstraintSet.always().implies_subtype_of(Invariant[T], Invariant[str]))

    # These are vacuously true; false implies anything
    static_assert(ConstraintSet.never().implies_subtype_of(Invariant[T], Invariant[int]))
    static_assert(ConstraintSet.never().implies_subtype_of(Invariant[T], Invariant[bool]))
    static_assert(ConstraintSet.never().implies_subtype_of(Invariant[T], Invariant[str]))

    # For an invariant typevar, (T ≤ int) does not imply that (Invariant[T] ≤ Invariant[int]).
    given_int = ConstraintSet.range(Never, T, int)
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[int]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[bool]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[str]))

    # It also does not imply the contravariant ordering (Invariant[int] ≤ Invariant[T]).
    static_assert(not given_int.implies_subtype_of(Invariant[int], Invariant[T]))
    static_assert(not given_int.implies_subtype_of(Invariant[bool], Invariant[T]))
    static_assert(not given_int.implies_subtype_of(Invariant[str], Invariant[T]))

    # But (T = int) does imply both.
    given_int = ConstraintSet.range(int, T, int)
    static_assert(given_int.implies_subtype_of(Invariant[T], Invariant[int]))
    static_assert(given_int.implies_subtype_of(Invariant[int], Invariant[T]))
    static_assert(not given_int.implies_subtype_of(Invariant[bool], Invariant[T]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[bool]))
    static_assert(not given_int.implies_subtype_of(Invariant[str], Invariant[T]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[str]))

def mutually_constrained[T, U]():
    # If (T = U ∧ U ≤ int), then (T ≤ int) must be true as well. But because T is invariant, that
    # does _not_ imply that (Invariant[T] ≤ Invariant[int]).
    given_int = ConstraintSet.range(U, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[int]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[bool]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[str]))

    # If (T = U ∧ U = int), then (T = int) must be true as well. That is an equality constraint, so
    # even though T is invariant, it does imply that (Invariant[T] ≤ Invariant[int]).
    given_int = ConstraintSet.range(U, T, U) & ConstraintSet.range(int, U, int)
    static_assert(given_int.implies_subtype_of(Invariant[T], Invariant[int]))
    static_assert(given_int.implies_subtype_of(Invariant[int], Invariant[T]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[bool]))
    static_assert(not given_int.implies_subtype_of(Invariant[bool], Invariant[T]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[str]))
    static_assert(not given_int.implies_subtype_of(Invariant[str], Invariant[T]))

# Repeat the test with a different typevar ordering
def mutually_constrained[U, T]():
    # If (T = U ∧ U ≤ int), then (T ≤ int) must be true as well. But because T is invariant, that
    # does _not_ imply that (Invariant[T] ≤ Invariant[int]).
    given_int = ConstraintSet.range(U, T, U) & ConstraintSet.range(Never, U, int)
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[int]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[bool]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[str]))

    # If (T = U ∧ U = int), then (T = int) must be true as well. That is an equality constraint, so
    # even though T is invariant, it does imply that (Invariant[T] ≤ Invariant[int]).
    given_int = ConstraintSet.range(U, T, U) & ConstraintSet.range(int, U, int)
    static_assert(given_int.implies_subtype_of(Invariant[T], Invariant[int]))
    static_assert(given_int.implies_subtype_of(Invariant[int], Invariant[T]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[bool]))
    static_assert(not given_int.implies_subtype_of(Invariant[bool], Invariant[T]))
    static_assert(not given_int.implies_subtype_of(Invariant[T], Invariant[str]))
    static_assert(not given_int.implies_subtype_of(Invariant[str], Invariant[T]))
```

## Generic callables

A generic callable can be considered equivalent to an intersection of all of its possible
specializations. That means that a generic callable is a subtype of any particular specialization.
(If someone expects a function that works with a particular specialization, it's fine to hand them
the generic callable.)

```py
from typing import Callable
from ty_extensions import CallableTypeOf, ConstraintSet, TypeOf, is_subtype_of, static_assert

def identity[T](t: T) -> T:
    return t

type GenericIdentity[T] = Callable[[T], T]

constraints = ConstraintSet.always()

static_assert(constraints.implies_subtype_of(TypeOf[identity], Callable[[int], int]))
static_assert(constraints.implies_subtype_of(TypeOf[identity], Callable[[str], str]))
static_assert(not constraints.implies_subtype_of(TypeOf[identity], Callable[[str], int]))

static_assert(constraints.implies_subtype_of(CallableTypeOf[identity], Callable[[int], int]))
static_assert(constraints.implies_subtype_of(CallableTypeOf[identity], Callable[[str], str]))
static_assert(not constraints.implies_subtype_of(CallableTypeOf[identity], Callable[[str], int]))

static_assert(constraints.implies_subtype_of(TypeOf[identity], GenericIdentity[int]))
static_assert(constraints.implies_subtype_of(TypeOf[identity], GenericIdentity[str]))
# This gives us the default specialization, GenericIdentity[Unknown], which does
# not participate in subtyping.
static_assert(not constraints.implies_subtype_of(TypeOf[identity], GenericIdentity))
```

The reverse is not true — if someone expects a generic function that can be called with any
specialization, we cannot hand them a function that only works with one specialization.

```py
static_assert(not constraints.implies_subtype_of(Callable[[int], int], TypeOf[identity]))
static_assert(not constraints.implies_subtype_of(Callable[[str], str], TypeOf[identity]))
static_assert(not constraints.implies_subtype_of(Callable[[str], int], TypeOf[identity]))

static_assert(not constraints.implies_subtype_of(Callable[[int], int], CallableTypeOf[identity]))
static_assert(not constraints.implies_subtype_of(Callable[[str], str], CallableTypeOf[identity]))
static_assert(not constraints.implies_subtype_of(Callable[[str], int], CallableTypeOf[identity]))

static_assert(not constraints.implies_subtype_of(GenericIdentity[int], TypeOf[identity]))
static_assert(not constraints.implies_subtype_of(GenericIdentity[str], TypeOf[identity]))
# This gives us the default specialization, GenericIdentity[Unknown], which does
# not participate in subtyping.
static_assert(not constraints.implies_subtype_of(GenericIdentity, TypeOf[identity]))
```

Unrelated typevars in the constraint set do not affect whether the subtyping check succeeds or
fails.

```py
def unrelated[T]():
    # Note that even though this typevar is also named T, it is not the same typevar as T@identity!
    constraints = ConstraintSet.range(bool, T, int)

    static_assert(constraints.implies_subtype_of(TypeOf[identity], Callable[[int], int]))
    static_assert(constraints.implies_subtype_of(TypeOf[identity], Callable[[str], str]))
    static_assert(not constraints.implies_subtype_of(TypeOf[identity], Callable[[str], int]))
    static_assert(constraints.implies_subtype_of(TypeOf[identity], GenericIdentity[int]))
    static_assert(constraints.implies_subtype_of(TypeOf[identity], GenericIdentity[str]))

    static_assert(not constraints.implies_subtype_of(Callable[[int], int], TypeOf[identity]))
    static_assert(not constraints.implies_subtype_of(Callable[[str], str], TypeOf[identity]))
    static_assert(not constraints.implies_subtype_of(Callable[[str], int], TypeOf[identity]))
    static_assert(not constraints.implies_subtype_of(GenericIdentity[int], TypeOf[identity]))
    static_assert(not constraints.implies_subtype_of(GenericIdentity[str], TypeOf[identity]))
```

The generic callable's typevar _also_ does not affect whether the subtyping check succeeds or fails!

```py
def identity2[T](t: T) -> T:
    # This constraint set refers to the same typevar as the generic function types below!
    constraints = ConstraintSet.range(bool, T, int)

    static_assert(constraints.implies_subtype_of(TypeOf[identity2], Callable[[int], int]))
    static_assert(constraints.implies_subtype_of(TypeOf[identity2], Callable[[str], str]))
    # TODO: no error
    # error: [static-assert-error]
    static_assert(not constraints.implies_subtype_of(TypeOf[identity2], Callable[[str], int]))
    static_assert(constraints.implies_subtype_of(TypeOf[identity2], GenericIdentity[int]))
    static_assert(constraints.implies_subtype_of(TypeOf[identity2], GenericIdentity[str]))

    static_assert(not constraints.implies_subtype_of(Callable[[int], int], TypeOf[identity2]))
    static_assert(not constraints.implies_subtype_of(Callable[[str], str], TypeOf[identity2]))
    static_assert(not constraints.implies_subtype_of(Callable[[str], int], TypeOf[identity2]))
    static_assert(not constraints.implies_subtype_of(GenericIdentity[int], TypeOf[identity2]))
    static_assert(not constraints.implies_subtype_of(GenericIdentity[str], TypeOf[identity2]))

    return t
```

[subtyping]: https://typing.python.org/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
