# Quantification

Quantification describes when an expression involving some type variables holds for the type
variables that remain. `exists` requires at least one valid assignment to the quantified variables;
`for_all` requires every valid assignment to satisfy the expression.

| Case | Formula                          | Expected result                             |
| ---- | -------------------------------- | ------------------------------------------- |
| C0   | `∃X. X = int ∧ A ≤ Invariant[X]` | Equivalent to `A ≤ Invariant[int]`          |
| E1   | `∃X. U ≤ X ∧ X = V`              | Equivalent to `U ≤ V`                       |
| E2   | `∃X. A ≤ Invariant[X] ∧ X ≤ B`   | `A` and `B` must admit a common `X`         |
| E3   | `∃X. A ≤ X ∧ Invariant[X] ≤ B`   | `A` and `B` must admit a common `X`         |
| E4   | `∃X. C₁(X, Y) ∧ C₂(X, Z)`        | Solutions for `Y` and `Z` remain correlated |
| E5   | `∃X ∈ {int, str}. C(X, Y, Z)`    | Solutions remain paired with each choice    |
| E6   | `∀T ∈ Dₜ. ∃S ∈ Dₛ. R(S, T)`      | `S` may depend on the choice of `T`         |

```toml
[environment]
python-version = "3.13"
```

## C0: grounded invariant

If `X` must equal `int`, then `∃X. X = int ∧ A ≤ Invariant[X]` is equivalent to
`A ≤ Invariant[int]`. The negated expressions are equivalent as well.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Invariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def set(self, value: T) -> None: ...

def grounded[X, A]() -> None:
    body = ConstraintSet.range(int, X, int) & ConstraintSet.range(Never, A, Invariant[X])
    quantified = body.exists(tuple[X])
    expected = ConstraintSet.range(Never, A, Invariant[int])

    static_assert(quantified == expected)
    static_assert(~quantified == ~expected)

    # revealed: tuple[Solution[X=int, A=Never]]
    reveal_type(body.solutions(inferable=tuple[X, A]))
    # revealed: tuple[Solution[A=Never]]
    reveal_type(quantified.solutions(inferable=tuple[A]))
```

## E1: relational bridge

There is an `X` satisfying `U ≤ X ∧ X = V` exactly when `U ≤ V`. The negated expressions are
equivalent as well.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def relational_bridge[X, U, V]() -> None:
    body = ConstraintSet.range(Never, U, X) & ConstraintSet.range(V, X, V)
    quantified = body.exists(tuple[X])
    expected = ConstraintSet.range(Never, U, V)

    static_assert(quantified == expected)
    static_assert(~quantified == ~expected)

    # revealed: tuple[Solution[V=U@relational_bridge, U=Never]]
    reveal_type(quantified.solutions(inferable=tuple[U, V]))
```

## E2: open invariant inverse image

A specialization satisfies `∃X. A ≤ Invariant[X] ∧ X ≤ B` only if there is some `X` compatible with
both `A` and `B`. `A = Invariant[str]` and `B ≤ int` cannot satisfy the expression, so they must
satisfy its negation.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Invariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def set(self, value: T) -> None: ...

def inverse_image[X, A, B]() -> None:
    body = ConstraintSet.range(Never, A, Invariant[X]) & ConstraintSet.range(Never, X, B)
    quantified = body.exists(tuple[X])
    invalid = ConstraintSet.range(Invariant[str], A, object) & ConstraintSet.range(Never, B, int)

    # revealed: tuple[Solution[A=Never, B=X@inverse_image, X=Never]]
    reveal_type(body.solutions(inferable=tuple[X, A, B]))
    # TODO: Quantifying X should leave restrictions on A and B.
    # revealed: tuple[()]
    reveal_type(quantified.solutions(inferable=tuple[A, B]))

    # The original expression has no solution for this specialization.
    # revealed: None
    reveal_type((body & invalid).solutions(inferable=tuple[X, A, B]))
    # TODO: The quantified expression should also have no solution for this specialization.
    # revealed: tuple[Solution[A=Invariant[str], B=Never]]
    reveal_type((quantified & invalid).solutions(inferable=tuple[A, B]))

    # TODO: The positive expression should reject this specialization, and its negation should accept it.
    static_assert((quantified & invalid) == ConstraintSet.never())  # error: [static-assert-error]
    static_assert((~quantified & invalid) == invalid)  # error: [static-assert-error]
```

## E3: witness-sensitive image

For `∃X. A ≤ X ∧ Invariant[X] ≤ B`, each choice of `X` determines which values of `A` and `B` can
satisfy the expression. `A ≥ int` and `B ≤ Invariant[str]` cannot satisfy it, so they must satisfy
its negation.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Invariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def set(self, value: T) -> None: ...

def witness_sensitive[X, A, B]() -> None:
    body = ConstraintSet.range(A, X, object) & ConstraintSet.range(Invariant[X], B, object)
    quantified = body.exists(tuple[X])
    invalid = ConstraintSet.range(int, A, object) & ConstraintSet.range(Never, B, Invariant[str])

    # A solution for B depends on the compatible choice of X.
    # revealed: tuple[Solution[X=A@witness_sensitive, A=X@witness_sensitive, B=Invariant[X@witness_sensitive]]]
    reveal_type(body.solutions(inferable=tuple[X, A, B]))
    # TODO: Quantifying X should leave restrictions on A and B.
    # revealed: tuple[()]
    reveal_type(quantified.solutions(inferable=tuple[A, B]))

    # revealed: None
    reveal_type((body & invalid).solutions(inferable=tuple[X, A, B]))
    # TODO: The quantified expression should also have no solution for this specialization.
    # revealed: tuple[Solution[A=int, B=Never]]
    reveal_type((quantified & invalid).solutions(inferable=tuple[A, B]))

    # TODO: The positive expression should reject this specialization, and its negation should accept it.
    static_assert((quantified & invalid) == ConstraintSet.never())  # error: [static-assert-error]
    static_assert((~quantified & invalid) == invalid)  # error: [static-assert-error]
```

## E4: correlated visible outputs

The two valid solution families are `(Y = int, Z = Invariant[int])` and
`(Y = str, Z = Invariant[str])`. The cross-pairing `(Y = int, Z = Invariant[str])` is invalid, so
quantifying `X` and negating the result must both retain the correlation between `Y` and `Z`.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Invariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def set(self, value: T) -> None: ...

def correlated_outputs[X, Y, Z]() -> None:
    int_family = (
        ConstraintSet.range(int, X, int)
        & ConstraintSet.range(int, Y, int)
        & ConstraintSet.range(Invariant[int], Z, Invariant[int])
    )
    str_family = (
        ConstraintSet.range(str, X, str)
        & ConstraintSet.range(str, Y, str)
        & ConstraintSet.range(Invariant[str], Z, Invariant[str])
    )
    body = int_family | str_family
    quantified = body.exists(tuple[X])
    expected = (ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[int], Z, Invariant[int])) | (
        ConstraintSet.range(str, Y, str) & ConstraintSet.range(Invariant[str], Z, Invariant[str])
    )
    invalid_cross = ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[str], Z, Invariant[str])

    static_assert(quantified == expected)
    static_assert(~quantified == ~expected)
    static_assert((quantified & invalid_cross) == ConstraintSet.never())

    # revealed: tuple[Solution[X=int | Y@correlated_outputs, Y=int, Z=Invariant[int]], Solution[X=str | Y@correlated_outputs, Y=str, Z=Invariant[str]]]
    reveal_type(body.solutions(inferable=tuple[X, Y, Z]))
    # revealed: tuple[Solution[Y=int, Z=Invariant[int]], Solution[Y=str, Z=Invariant[str]]]
    reveal_type(quantified.solutions(inferable=tuple[Y, Z]))
    # revealed: None
    reveal_type((quantified & invalid_cross).solutions(inferable=tuple[Y, Z]))
```

## E5: finite domain

When `X` is either `int` or `str`, each choice gives a separate valid solution family. After `X` is
quantified, `Y` and `Z` must remain paired with the same choice; the cross-pairing is invalid.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Invariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def set(self, value: T) -> None: ...

def finite_domain[X: (int, str), Y, Z]() -> None:
    x_int = ConstraintSet.range(int, X, int)
    x_str = ConstraintSet.range(str, X, str)
    domain = x_int | x_str
    body = (x_int & ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[int], Z, Invariant[int])) | (
        x_str & ConstraintSet.range(str, Y, str) & ConstraintSet.range(Invariant[str], Z, Invariant[str])
    )
    quantified = (domain & body).exists(tuple[X])
    expected = (ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[int], Z, Invariant[int])) | (
        ConstraintSet.range(str, Y, str) & ConstraintSet.range(Invariant[str], Z, Invariant[str])
    )
    invalid_cross = ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[str], Z, Invariant[str])

    static_assert(quantified == expected)
    static_assert(~quantified == ~expected)
    static_assert((quantified & invalid_cross) == ConstraintSet.never())

    # revealed: tuple[Solution[X=int, Y=int, Z=Invariant[int]], Solution[X=str, Y=str, Z=Invariant[str]]]
    reveal_type((domain & body).solutions(inferable=tuple[X, Y, Z]))
    # revealed: tuple[Solution[Y=int, Z=Invariant[int]], Solution[Y=str, Z=Invariant[str]]]
    reveal_type(quantified.solutions(inferable=tuple[Y, Z]))
    # revealed: None
    reveal_type((quantified & invalid_cross).solutions(inferable=tuple[Y, Z]))
```

## E6: alternation and negative polarity

For every valid choice of `T`, there is a matching choice of `S`. Reversing the quantifiers would
require one choice of `S` to work for every `T` and is therefore false. Negating the relation asks
whether there is a `T` with no matching `S`; an `int`-only relation shows that a missing `str` case
is rejected.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def alternation[S, T]() -> None:
    s_int = ConstraintSet.range(int, S, int)
    s_str = ConstraintSet.range(str, S, str)
    t_int = ConstraintSet.range(int, T, int)
    t_str = ConstraintSet.range(str, T, str)
    source_domain = s_int | s_str
    target_domain = t_int | t_str
    relation = (s_int & t_int) | (s_str & t_str)

    source_exists = (source_domain & relation).exists(tuple[S])
    forall_target_exists_source = target_domain.satisfies(source_exists).for_all(tuple[T])
    exists_source_forall_target = (source_domain & target_domain.satisfies(relation).for_all(tuple[T])).exists(tuple[S])

    static_assert(forall_target_exists_source == ConstraintSet.always())
    static_assert(~forall_target_exists_source == ConstraintSet.never())
    static_assert(exists_source_forall_target == ConstraintSet.never())

    source_has_no_witness = (~(source_domain & relation)).for_all(tuple[S])
    target_counterexample = (target_domain & source_has_no_witness).exists(tuple[T])
    static_assert(target_counterexample == ConstraintSet.never())
    static_assert(target_counterexample == ~forall_target_exists_source)

    int_only = s_int & t_int
    int_source_exists = (source_domain & int_only).exists(tuple[S])
    missing_str = target_domain.satisfies(int_source_exists).for_all(tuple[T])
    static_assert(missing_str == ConstraintSet.never())
```
