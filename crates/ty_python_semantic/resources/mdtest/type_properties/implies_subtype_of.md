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
from ty_extensions import RegularCallableTypeOf, ConstraintSet, TypeOf, is_subtype_of, static_assert

def identity[T](t: T) -> T:
    return t

type GenericIdentity[T] = Callable[[T], T]

constraints = ConstraintSet.always()

static_assert(constraints.implies_subtype_of(TypeOf[identity], Callable[[int], int]))
static_assert(constraints.implies_subtype_of(TypeOf[identity], Callable[[str], str]))
static_assert(not constraints.implies_subtype_of(TypeOf[identity], Callable[[str], int]))

static_assert(constraints.implies_subtype_of(RegularCallableTypeOf[identity], Callable[[int], int]))
static_assert(constraints.implies_subtype_of(RegularCallableTypeOf[identity], Callable[[str], str]))
static_assert(not constraints.implies_subtype_of(RegularCallableTypeOf[identity], Callable[[str], int]))

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

static_assert(not constraints.implies_subtype_of(Callable[[int], int], RegularCallableTypeOf[identity]))
static_assert(not constraints.implies_subtype_of(Callable[[str], str], RegularCallableTypeOf[identity]))
static_assert(not constraints.implies_subtype_of(Callable[[str], int], RegularCallableTypeOf[identity]))

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

## Transitivity

### Transitivity can propagate across typevars

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

def concrete_pivot[T, U]():
    # If [int ≤ T ∧ T ≤ U], then [int ≤ U] must be true as well.
    constraints = ConstraintSet.range(int, T, object) & ConstraintSet.range(T, U, object)
    static_assert(constraints.implies_subtype_of(int, U))
```

### Transitivity can propagate across fully static concrete types

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

def concrete_pivot[T, U]():
    # If [T ≤ int ∧ int ≤ U], then [T ≤ U] must be true as well.
    constraints = ConstraintSet.range(Never, T, int) & ConstraintSet.range(int, U, object)
    static_assert(constraints.implies_subtype_of(T, U))
```

### Transitivity cannot propagate across non-fully-static concrete types

```py
from typing import Any, Never
from ty_extensions import ConstraintSet, static_assert

def concrete_pivot[T, U]():
    # If [T ≤ Any ∧ Any ≤ U], then the two `Any`s might materialize to different types. That means
    # [T ≤ U] is NOT necessarily true.
    constraints = ConstraintSet.range(Never, T, Any) & ConstraintSet.range(Any, U, object)
    static_assert(not constraints.implies_subtype_of(T, U))
```

### Transitivity can propagate through nested covariant typevars

When a typevar appears nested inside a covariant generic type in another constraint's bound, we can
propagate the bound "into" the generic type.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Covariant[T]:
    def get(self) -> T:
        raise ValueError

def upper_bound[T, U]():
    # If (T ≤ int) ∧ (U ≤ Covariant[T]), then by covariance, Covariant[T] ≤ Covariant[int],
    # and by transitivity, U ≤ Covariant[int].
    constraints = ConstraintSet.range(Never, T, int) & ConstraintSet.range(Never, U, Covariant[T])
    static_assert(constraints.implies_subtype_of(U, Covariant[int]))
    static_assert(not constraints.implies_subtype_of(U, Covariant[bool]))
    static_assert(not constraints.implies_subtype_of(U, Covariant[str]))

def lower_bound[T, U]():
    # If (int ≤ T ∧ Covariant[T] ≤ U), then by covariance, Covariant[int] ≤ Covariant[T],
    # and by transitivity, Covariant[int] ≤ U. Since bool ≤ int, Covariant[bool] ≤ U also holds.
    constraints = ConstraintSet.range(int, T, object) & ConstraintSet.range(Covariant[T], U, object)
    static_assert(constraints.implies_subtype_of(Covariant[int], U))
    static_assert(constraints.implies_subtype_of(Covariant[bool], U))
    static_assert(not constraints.implies_subtype_of(Covariant[str], U))

# Repeat with reversed typevar ordering to verify BDD-ordering independence.
def upper_bound[U, T]():
    constraints = ConstraintSet.range(Never, T, int) & ConstraintSet.range(Never, U, Covariant[T])
    static_assert(constraints.implies_subtype_of(U, Covariant[int]))
    static_assert(not constraints.implies_subtype_of(U, Covariant[bool]))
    static_assert(not constraints.implies_subtype_of(U, Covariant[str]))

def lower_bound[U, T]():
    # Since bool ≤ int, Covariant[bool] ≤ U also holds.
    constraints = ConstraintSet.range(int, T, object) & ConstraintSet.range(Covariant[T], U, object)
    static_assert(constraints.implies_subtype_of(Covariant[int], U))
    static_assert(constraints.implies_subtype_of(Covariant[bool], U))
    static_assert(not constraints.implies_subtype_of(Covariant[str], U))
```

### Transitivity can propagate through nested contravariant typevars

The previous section also works for contravariant generic types, though one of the antecedent
constraints is flipped.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Contravariant[T]:
    def set(self, value: T):
        pass

def upper_bound[T, U]():
    # If (int ≤ T ∧ U ≤ Contravariant[T]), then by contravariance,
    # Contravariant[T] ≤ Contravariant[int], and by transitivity, U ≤ Contravariant[int].
    # Note: we need the *lower* bound on T (not the upper) because contravariance flips.
    # Since bool ≤ int, Contravariant[int] ≤ Contravariant[bool], so U ≤ Contravariant[bool]
    # also holds.
    constraints = ConstraintSet.range(int, T, object) & ConstraintSet.range(Never, U, Contravariant[T])
    static_assert(constraints.implies_subtype_of(U, Contravariant[int]))
    static_assert(constraints.implies_subtype_of(U, Contravariant[bool]))
    static_assert(not constraints.implies_subtype_of(U, Contravariant[str]))

def lower_bound[T, U]():
    # If (T ≤ int ∧ Contravariant[T] ≤ U), then by contravariance,
    # Contravariant[int] ≤ Contravariant[T], and by transitivity, Contravariant[int] ≤ U.
    # Contravariant[bool] is a supertype of Contravariant[int] (since bool ≤ int), so
    # Contravariant[bool] ≤ U does NOT hold.
    constraints = ConstraintSet.range(Never, T, int) & ConstraintSet.range(Contravariant[T], U, object)
    static_assert(constraints.implies_subtype_of(Contravariant[int], U))
    static_assert(not constraints.implies_subtype_of(Contravariant[bool], U))
    static_assert(not constraints.implies_subtype_of(Contravariant[str], U))

# Repeat with reversed typevar ordering to verify BDD-ordering independence.
def upper_bound[U, T]():
    constraints = ConstraintSet.range(int, T, object) & ConstraintSet.range(Never, U, Contravariant[T])
    static_assert(constraints.implies_subtype_of(U, Contravariant[int]))
    static_assert(constraints.implies_subtype_of(U, Contravariant[bool]))
    static_assert(not constraints.implies_subtype_of(U, Contravariant[str]))

def lower_bound[U, T]():
    constraints = ConstraintSet.range(Never, T, int) & ConstraintSet.range(Contravariant[T], U, object)
    static_assert(constraints.implies_subtype_of(Contravariant[int], U))
    static_assert(not constraints.implies_subtype_of(Contravariant[bool], U))
    static_assert(not constraints.implies_subtype_of(Contravariant[str], U))
```

### Transitivity can propagate through nested invariant typevars

For invariant type parameters, only an equality constraint on the typevar allows propagation. A
one-sided bound (upper or lower only) is not sufficient.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Invariant[T]:
    def get(self) -> T:
        raise ValueError

    def set(self, value: T):
        pass

def equality_constraint[T, U]():
    # (T = int ∧ U ≤ Invariant[T]) should imply U ≤ Invariant[int].
    constraints = ConstraintSet.range(int, T, int) & ConstraintSet.range(Never, U, Invariant[T])
    static_assert(constraints.implies_subtype_of(U, Invariant[int]))
    static_assert(not constraints.implies_subtype_of(U, Invariant[bool]))
    static_assert(not constraints.implies_subtype_of(U, Invariant[str]))

def upper_bound_only[T, U]():
    # (T ≤ int ∧ U ≤ Invariant[T]) should NOT imply U ≤ Invariant[int], because T is invariant
    # and we only have an upper bound, not equality.
    constraints = ConstraintSet.range(Never, T, int) & ConstraintSet.range(Never, U, Invariant[T])
    static_assert(not constraints.implies_subtype_of(U, Invariant[int]))
    static_assert(not constraints.implies_subtype_of(U, Invariant[bool]))
    static_assert(not constraints.implies_subtype_of(U, Invariant[str]))

def lower_bound_only[T, U]():
    # (int ≤ T ∧ Invariant[T] ≤ U) should NOT imply Invariant[int] ≤ U, because T is invariant
    # and we only have a lower bound, not equality.
    constraints = ConstraintSet.range(int, T, object) & ConstraintSet.range(Invariant[T], U, object)
    static_assert(not constraints.implies_subtype_of(Invariant[int], U))
    static_assert(not constraints.implies_subtype_of(Invariant[bool], U))
    static_assert(not constraints.implies_subtype_of(Invariant[str], U))

# Repeat with reversed typevar ordering.
def equality_constraint[U, T]():
    constraints = ConstraintSet.range(int, T, int) & ConstraintSet.range(Never, U, Invariant[T])
    static_assert(constraints.implies_subtype_of(U, Invariant[int]))
    static_assert(not constraints.implies_subtype_of(U, Invariant[bool]))
    static_assert(not constraints.implies_subtype_of(U, Invariant[str]))
```

### Transitivity propagates through composed variance

When a typevar is nested inside multiple layers of generics, variances compose. For instance, a
covariant type inside a contravariant type yields contravariant overall.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Covariant[T]:
    def get(self) -> T:
        raise ValueError

class Contravariant[T]:
    def set(self, value: T):
        pass

def covariant_of_contravariant[T, U]():
    # Covariant[Contravariant[T]]: T is contravariant overall (covariant × contravariant).
    # So a lower bound on T should propagate (flipped).
    constraints = ConstraintSet.range(int, T, object) & ConstraintSet.range(Never, U, Covariant[Contravariant[T]])
    static_assert(constraints.implies_subtype_of(U, Covariant[Contravariant[int]]))
    static_assert(not constraints.implies_subtype_of(U, Covariant[Contravariant[str]]))

def contravariant_of_covariant[T, U]():
    # Contravariant[Covariant[T]]: T is contravariant overall (contravariant × covariant).
    # So a lower bound on T should propagate (flipped).
    constraints = ConstraintSet.range(int, T, object) & ConstraintSet.range(Never, U, Contravariant[Covariant[T]])
    static_assert(constraints.implies_subtype_of(U, Contravariant[Covariant[int]]))
    static_assert(not constraints.implies_subtype_of(U, Contravariant[Covariant[str]]))

# Repeat with reversed typevar ordering.
def covariant_of_contravariant[U, T]():
    constraints = ConstraintSet.range(int, T, object) & ConstraintSet.range(Never, U, Covariant[Contravariant[T]])
    static_assert(constraints.implies_subtype_of(U, Covariant[Contravariant[int]]))
    static_assert(not constraints.implies_subtype_of(U, Covariant[Contravariant[str]]))

def contravariant_of_covariant[U, T]():
    constraints = ConstraintSet.range(int, T, object) & ConstraintSet.range(Never, U, Contravariant[Covariant[T]])
    static_assert(constraints.implies_subtype_of(U, Contravariant[Covariant[int]]))
    static_assert(not constraints.implies_subtype_of(U, Contravariant[Covariant[str]]))
```

### Typevar bound substitution into nested generic types

When a typevar B has a bare typevar S as one of its bounds, and S appears nested inside another
constraint's bound, we can substitute B for S to create a cross-typevar link. The derived constraint
is weaker (less restrictive), but introduces a useful relationship between the typevars.

For example, `(Covariant[S] ≤ C) ∧ (S ≤ B)` should imply `Covariant[B] ≤ C`: we are given that
`S ≤ B`, covariance tells us that `Covariant[S] ≤ Covariant[B]`, and transitivity gives
`Covariant[B] ≤ C`. (We can infer similar weakened constraints for contravariant and invariant
typevars.)

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Covariant[T]:
    def get(self) -> T:
        raise ValueError

class Contravariant[T]:
    def set(self, value: T):
        pass

class Invariant[T]:
    def get(self) -> T:
        raise ValueError

    def set(self, value: T):
        pass

def covariant_upper_bound_into_lower[S, B, C]():
    # (Covariant[S] ≤ C) ∧ (B ≤ S) → (Covariant[B] ≤ C)
    # B ≤ S, so Covariant[B] ≤ Covariant[S], and Covariant[S] ≤ C gives Covariant[B] ≤ C.
    constraints = ConstraintSet.range(Covariant[S], C, object) & ConstraintSet.range(Never, B, S)
    static_assert(constraints.implies_subtype_of(Covariant[B], C))

def covariant_lower_bound_into_upper[S, B, C]():
    # (C ≤ Covariant[S]) ∧ (S ≤ B) → (C ≤ Covariant[B])
    # S ≤ B, so Covariant[S] ≤ Covariant[B], and C ≤ Covariant[S] ≤ Covariant[B].
    constraints = ConstraintSet.range(Never, C, Covariant[S]) & ConstraintSet.range(S, B, object)
    static_assert(constraints.implies_subtype_of(C, Covariant[B]))

def contravariant_upper_bound_into_lower[S, B, C]():
    # (Contravariant[S] ≤ C) ∧ (S ≤ B) → (Contravariant[B] ≤ C)
    # S ≤ B gives Contravariant[B] ≤ Contravariant[S], so Contravariant[B] ≤ Contravariant[S] ≤ C.
    constraints = ConstraintSet.range(Contravariant[S], C, object) & ConstraintSet.range(S, B, object)
    static_assert(constraints.implies_subtype_of(Contravariant[B], C))

def contravariant_lower_bound_into_upper[S, B, C]():
    # (C ≤ Contravariant[S]) ∧ (B ≤ S) → (C ≤ Contravariant[B])
    # B ≤ S gives Contravariant[S] ≤ Contravariant[B], so C ≤ Contravariant[S] ≤ Contravariant[B].
    constraints = ConstraintSet.range(Never, C, Contravariant[S]) & ConstraintSet.range(Never, B, S)
    static_assert(constraints.implies_subtype_of(C, Contravariant[B]))

# Repeat with reversed typevar ordering.
def covariant_upper_bound_into_lower[C, B, S]():
    constraints = ConstraintSet.range(Covariant[S], C, object) & ConstraintSet.range(Never, B, S)
    static_assert(constraints.implies_subtype_of(Covariant[B], C))

def covariant_lower_bound_into_upper[C, B, S]():
    constraints = ConstraintSet.range(Never, C, Covariant[S]) & ConstraintSet.range(S, B, object)
    static_assert(constraints.implies_subtype_of(C, Covariant[B]))

def contravariant_upper_bound_into_lower[C, B, S]():
    constraints = ConstraintSet.range(Contravariant[S], C, object) & ConstraintSet.range(S, B, object)
    static_assert(constraints.implies_subtype_of(Contravariant[B], C))

def contravariant_lower_bound_into_upper[C, B, S]():
    constraints = ConstraintSet.range(Never, C, Contravariant[S]) & ConstraintSet.range(Never, B, S)
    static_assert(constraints.implies_subtype_of(C, Contravariant[B]))
```

### Concrete bound substitution into nested generic types (future extension)

When B's bound _contains_ a typevar (but is not a bare typevar), the same logic as above applies.

TODO: This is not implemented yet, since it requires different detection machinery.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Covariant[T]:
    def get(self) -> T:
        raise ValueError

def upper_bound_into_lower[B, C]():
    # (Covariant[int] ≤ C) ∧ (B ≤ int) → (Covariant[B] ≤ C)
    constraints = ConstraintSet.range(Covariant[int], C, object) & ConstraintSet.range(Never, B, int)
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.implies_subtype_of(Covariant[B], C))

def lower_bound_into_upper[B, C]():
    # (C ≤ Covariant[int]) ∧ (int ≤ B) → (C ≤ Covariant[B])
    constraints = ConstraintSet.range(Never, C, Covariant[int]) & ConstraintSet.range(int, B, object)
    # TODO: no error
    # error: [static-assert-error]
    static_assert(constraints.implies_subtype_of(C, Covariant[B]))
```

### Nested typevar propagation also works when the replacement is a bare typevar

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Covariant[T]:
    def get(self) -> T:
        raise ValueError

class Contravariant[T]:
    def set(self, value: T):
        pass

class Invariant[T]:
    def get(self) -> T:
        raise ValueError

    def set(self, value: T):
        pass

def covariant_upper[B, S, U]():
    # (B ≤ S) ∧ (U ≤ Covariant[B]) -> (U ≤ Covariant[S])
    constraints = ConstraintSet.range(Never, B, S) & ConstraintSet.range(Never, U, Covariant[B])
    static_assert(constraints.implies_subtype_of(U, Covariant[S]))

def covariant_lower[B, S, U]():
    # (S ≤ B) ∧ (Covariant[B] ≤ U) -> (Covariant[S] ≤ U)
    constraints = ConstraintSet.range(S, B, object) & ConstraintSet.range(Covariant[B], U, object)
    static_assert(constraints.implies_subtype_of(Covariant[S], U))

def contravariant_upper[B, S, U]():
    # (S ≤ B) ∧ (U ≤ Contravariant[B]) -> (U ≤ Contravariant[S])
    constraints = ConstraintSet.range(S, B, object) & ConstraintSet.range(Never, U, Contravariant[B])
    static_assert(constraints.implies_subtype_of(U, Contravariant[S]))

def contravariant_lower[B, S, U]():
    # (B ≤ S) ∧ (Contravariant[B] ≤ U) -> (Contravariant[S] ≤ U)
    constraints = ConstraintSet.range(Never, B, S) & ConstraintSet.range(Contravariant[B], U, object)
    static_assert(constraints.implies_subtype_of(Contravariant[S], U))

def invariant_upper_requires_equality[B, S, U]():
    # Invariant replacement only holds under equality constraints on B.
    constraints = ConstraintSet.range(S, B, S) & ConstraintSet.range(Never, U, Invariant[B])
    static_assert(constraints.implies_subtype_of(U, Invariant[S]))

def invariant_lower_requires_equality[B, S, U]():
    constraints = ConstraintSet.range(S, B, S) & ConstraintSet.range(Invariant[B], U, object)
    static_assert(constraints.implies_subtype_of(Invariant[S], U))

def invariant_upper_one_sided_is_not_enough[B, S, U]():
    constraints = ConstraintSet.range(Never, B, S) & ConstraintSet.range(Never, U, Invariant[B])
    static_assert(not constraints.implies_subtype_of(U, Invariant[S]))

def invariant_lower_one_sided_is_not_enough[B, S, U]():
    constraints = ConstraintSet.range(S, B, object) & ConstraintSet.range(Invariant[B], U, object)
    static_assert(not constraints.implies_subtype_of(Invariant[S], U))
```

### Reverse decomposition: bounds on a typevar can be decomposed via variance

When a constraint has lower and upper bounds that are parameterizations of the same generic type, we
can decompose the bounds to extract constraints on the nested typevar. For instance, the constraint
`Covariant[int] ≤ A ≤ Covariant[T]` implies `int ≤ T`, because `Covariant` is covariant and
`Covariant[int] ≤ Covariant[T]` requires `int ≤ T`.

```py
from typing import Never
from ty_extensions import ConstraintSet, static_assert

class Covariant[T]:
    def get(self) -> T:
        raise ValueError

class Contravariant[T]:
    def set(self, value: T):
        pass

class Invariant[T]:
    def get(self) -> T:
        raise ValueError

    def set(self, value: T):
        pass

def covariant_decomposition[A, T]():
    # Covariant[int] ≤ A ≤ Covariant[T] implies int ≤ T.
    constraints = ConstraintSet.range(Covariant[int], A, Covariant[T])
    static_assert(constraints.implies_subtype_of(int, T))
    static_assert(not constraints.implies_subtype_of(str, T))

def contravariant_decomposition[A, T]():
    # Contravariant[int] ≤ A ≤ Contravariant[T] implies T ≤ int (flipped).
    # Because contravariance reverses: Contravariant[int] ≤ Contravariant[T] means T ≤ int.
    constraints = ConstraintSet.range(Contravariant[int], A, Contravariant[T])
    static_assert(constraints.implies_subtype_of(T, int))
    static_assert(not constraints.implies_subtype_of(T, str))

def invariant_decomposition[A, T]():
    # Invariant[int] ≤ A ≤ Invariant[T] implies T = int.
    constraints = ConstraintSet.range(Invariant[int], A, Invariant[T])
    static_assert(constraints.implies_subtype_of(T, int))
    static_assert(constraints.implies_subtype_of(int, T))
    static_assert(not constraints.implies_subtype_of(T, str))

def bare_typevar_decomposition[A, S, T]():
    # S ≤ A ≤ T implies S ≤ T. This is existing behavior (bare typevar transitivity)
    # that should continue to work.
    constraints = ConstraintSet.range(S, A, T)
    static_assert(constraints.implies_subtype_of(S, T))

# Repeat with reversed typevar ordering.
def covariant_decomposition[T, A]():
    constraints = ConstraintSet.range(Covariant[int], A, Covariant[T])
    static_assert(constraints.implies_subtype_of(int, T))
    static_assert(not constraints.implies_subtype_of(str, T))

def contravariant_decomposition[T, A]():
    constraints = ConstraintSet.range(Contravariant[int], A, Contravariant[T])
    static_assert(constraints.implies_subtype_of(T, int))
    static_assert(not constraints.implies_subtype_of(T, str))

def invariant_decomposition[T, A]():
    constraints = ConstraintSet.range(Invariant[int], A, Invariant[T])
    static_assert(constraints.implies_subtype_of(T, int))
    static_assert(constraints.implies_subtype_of(int, T))
    static_assert(not constraints.implies_subtype_of(T, str))

# The lower and upper bounds don't need to be parameterizations of the same type — our inference
# logic handles subtyping naturally.
class Sub(Covariant[int]): ...

def subclass_lower_bound[A, T]():
    # Sub ≤ Covariant[int], so Sub ≤ A ≤ Covariant[T] implies int ≤ T.
    constraints = ConstraintSet.range(Sub, A, Covariant[T])
    static_assert(constraints.implies_subtype_of(int, T))
    static_assert(not constraints.implies_subtype_of(str, T))

def subclass_lower_bound[T, A]():
    constraints = ConstraintSet.range(Sub, A, Covariant[T])
    static_assert(constraints.implies_subtype_of(int, T))
    static_assert(not constraints.implies_subtype_of(str, T))
```

### Transitivity should not introduce impossible constraints

```py
from typing import Never, TypeVar, Union
from ty_extensions import ConstraintSet, static_assert

def impossible_result[A, T, U]():
    constraint_a = ConstraintSet.range(int, A, Union[T, U])
    constraint_t = ConstraintSet.range(Never, T, str)
    constraint_u = ConstraintSet.range(Never, U, bytes)

    # Given (int ≤ A ≤ T | U), we can infer that (int ≤ T) ∨ (int ≤ U). If we intersect that with
    # (T ≤ str), we get false ∨ (int ≤ U) — that is, there is no valid solution for T. Therefore A
    # cannot be a subtype of T; it must be a subtype of U.
    constraints = constraint_a & constraint_t
    static_assert(constraints.implies_subtype_of(int, U))

    # And if we intersect with (U ≤ bytes) as well, then there are no valid solutions for either T
    # or U, and the constraint set as a whole becomes unsatisfiable.
    constraints = constraint_a & constraint_t & constraint_u
    static_assert(not constraints)
```

[subtyping]: https://typing.python.org/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
