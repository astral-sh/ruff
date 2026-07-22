# Scoped quantifiers

These tests establish a small behavior basis for eliminating callable-local type variables. An
existential is written using the currently exposed universal-quantification hook and Boolean
duality: `∃X.C = ¬∀X.¬C`. This exercises the semantics without depending on how a future scoped
quantifier is represented internally. Revealed solution expectations use the stable default
constraint ordering; path and binding order can still vary when `TY_CONSTRAINT_SET_ORDER` is
perturbed (as documented in `regression/constraint_set_ordering.md`).

| Case | Formula                          | Expected result                   |
| ---- | -------------------------------- | --------------------------------- |
| C0   | `∃X. X = int ∧ A ≤ Invariant[X]` | Exact substitution                |
| E1   | `∃X. U ≤ X ∧ X = V`              | Exact relational cover            |
| E2   | `∃X. A ≤ Invariant[X] ∧ X ≤ B`   | Residual invariant relation       |
| E3   | `∃X. A ≤ X ∧ Invariant[X] ≤ B`   | Witness-sensitive residual        |
| E4   | `∃X. C₁(X, Y) ∧ C₂(X, Z)`        | Correlated visible solutions      |
| E5   | `∃X ∈ {int, str}. C(X, Y, Z)`    | Finite disjunctive cover          |
| E6   | `∀T ∈ Dₜ. ∃S ∈ Dₛ. R(S, T)`      | Preserve alternation and polarity |

```toml
[environment]
python-version = "3.13"
```

## C0: grounded invariant

An exact equality for the local witness permits substitution through an invariant constructor. The
result is an ordinary constraint on the visible variable, and negation preserves that cover.

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
    projected = ~((~body).for_all(tuple[X]))
    expected = ConstraintSet.range(Never, A, Invariant[int])

    static_assert(projected == expected)
    static_assert(~projected == ~expected)

    # revealed: tuple[Solution[X=int, A=Never]]
    reveal_type(body.solutions(inferable=tuple[X, A]))
    # revealed: tuple[Solution[A=Never]]
    reveal_type(projected.solutions(inferable=tuple[A]))
```

## E1: relational bridge

A local witness between two visible variables has an exact cover: `U ≤ X ∧ X = V` projects to
`U ≤ V`. No invariant residual is necessary.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def relational_bridge[X, U, V]() -> None:
    body = ConstraintSet.range(Never, U, X) & ConstraintSet.range(V, X, V)
    projected = ~((~body).for_all(tuple[X]))
    expected = ConstraintSet.range(Never, U, V)

    # TODO: These exact-cover assertions currently fail for some perturbed constraint orders,
    # despite both sides displaying as `U ≤ V`.
    static_assert(projected == expected)
    static_assert(~projected == ~expected)

    # revealed: tuple[Solution[V=U@relational_bridge, U=Never]]
    reveal_type(projected.solutions(inferable=tuple[U, V]))
```

## E2: open invariant inverse image

There is no known finite ordinary cover for `A ≤ Invariant[X] ∧ X ≤ B`. Keeping the relation scoped
is necessary: choosing `A = Invariant[str]` and `B ≤ int` cannot have a valid witness. Eager
projection currently drops both constraints involving `X`, admits the invalid specialization, and
also loses its negative-polarity counterpart.

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
    projected = ~((~body).for_all(tuple[X]))
    invalid = ConstraintSet.range(Invariant[str], A, object) & ConstraintSet.range(Never, B, int)

    # revealed: tuple[Solution[A=Never, B=X@inverse_image, X=Never]]
    reveal_type(body.solutions(inferable=tuple[X, A, B]))
    # TODO: A scoped residual should retain the relationship between A and B.
    # revealed: tuple[()]
    reveal_type(projected.solutions(inferable=tuple[A, B]))

    # Keeping X until the visible constraints are known correctly rejects the path.
    # revealed: None
    reveal_type((body & invalid).solutions(inferable=tuple[X, A, B]))
    # TODO: This should be `None`; eager projection incorrectly admits the path.
    # revealed: tuple[Solution[A=Invariant[str], B=Never]]
    reveal_type((projected & invalid).solutions(inferable=tuple[A, B]))

    # TODO: Both assertions should pass once the positive and negative residuals are preserved.
    static_assert((projected & invalid) == ConstraintSet.never())  # error: [static-assert-error]
    static_assert((~projected & invalid) == invalid)  # error: [static-assert-error]
```

## E3: witness-sensitive image

For `A ≤ X ∧ Invariant[X] ≤ B`, the chosen local witness determines the compatible visible upper
bound. The constraint layer must preserve that family until inference policy chooses a witness.
`A ≥ int` and `B ≤ Invariant[str]` demonstrate the same loss under eager projection and under its
negation.

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
    projected = ~((~body).for_all(tuple[X]))
    invalid = ConstraintSet.range(int, A, object) & ConstraintSet.range(Never, B, Invariant[str])

    # The visible bound for B still refers to the local witness on the complete path.
    # revealed: tuple[Solution[X=A@witness_sensitive, A=X@witness_sensitive, B=Invariant[X@witness_sensitive]]]
    reveal_type(body.solutions(inferable=tuple[X, A, B]))
    # TODO: A scoped residual should preserve the witness-sensitive relationship.
    # revealed: tuple[()]
    reveal_type(projected.solutions(inferable=tuple[A, B]))

    # revealed: None
    reveal_type((body & invalid).solutions(inferable=tuple[X, A, B]))
    # TODO: This should be `None`; eager projection incorrectly admits the path.
    # revealed: tuple[Solution[A=int, B=Never]]
    reveal_type((projected & invalid).solutions(inferable=tuple[A, B]))

    # TODO: Both assertions should pass once the positive and negative residuals are preserved.
    static_assert((projected & invalid) == ConstraintSet.never())  # error: [static-assert-error]
    static_assert((~projected & invalid) == invalid)  # error: [static-assert-error]
```

## E4: correlated visible outputs

Eliminating a common witness must not solve visible variables independently. The two valid families
are `(Y = int, Z = Invariant[int])` and `(Y = str, Z = Invariant[str])`; the cross-pairing
`(Y = int, Z = Invariant[str])` is invalid. The disjunctive cover and its negation retain that
correlation.

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
    projected = ~((~body).for_all(tuple[X]))
    expected = (ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[int], Z, Invariant[int])) | (
        ConstraintSet.range(str, Y, str) & ConstraintSet.range(Invariant[str], Z, Invariant[str])
    )
    invalid_cross = ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[str], Z, Invariant[str])

    static_assert(projected == expected)
    static_assert(~projected == ~expected)
    static_assert((projected & invalid_cross) == ConstraintSet.never())

    # revealed: tuple[Solution[X=int | Y@correlated_outputs, Y=int, Z=Invariant[int]], Solution[X=str | Y@correlated_outputs, Y=str, Z=Invariant[str]]]
    reveal_type(body.solutions(inferable=tuple[X, Y, Z]))
    # revealed: tuple[Solution[Y=int, Z=Invariant[int]], Solution[Y=str, Z=Invariant[str]]]
    reveal_type(projected.solutions(inferable=tuple[Y, Z]))
    # revealed: None
    reveal_type((projected & invalid_cross).solutions(inferable=tuple[Y, Z]))
```

## E5: finite domain

A finite constrained domain permits an exact disjunctive cover with branch-local witnesses. The
domain is included explicitly because the current `for_all` hook does not implicitly apply declared
TypeVar constraints. As in E4, the visible solutions remain paired after `X` is eliminated.

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
    projected = ~((~(domain & body)).for_all(tuple[X]))
    expected = (ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[int], Z, Invariant[int])) | (
        ConstraintSet.range(str, Y, str) & ConstraintSet.range(Invariant[str], Z, Invariant[str])
    )
    invalid_cross = ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[str], Z, Invariant[str])

    static_assert(projected == expected)
    static_assert(~projected == ~expected)
    static_assert((projected & invalid_cross) == ConstraintSet.never())

    # revealed: tuple[Solution[X=int, Y=int, Z=Invariant[int]], Solution[X=str, Y=str, Z=Invariant[str]]]
    reveal_type((domain & body).solutions(inferable=tuple[X, Y, Z]))
    # revealed: tuple[Solution[Y=int, Z=Invariant[int]], Solution[Y=str, Z=Invariant[str]]]
    reveal_type(projected.solutions(inferable=tuple[Y, Z]))
    # revealed: None
    reveal_type((projected & invalid_cross).solutions(inferable=tuple[Y, Z]))
```

## E6: alternation and negative polarity

For each valid target specialization, a matching source witness exists. Reversing the quantifiers
would require one source specialization to work for every target specialization and is therefore
false. The explicit counterexample formula checks the negative polarity of the same nested relation,
and an `int`-only relation confirms that a missing target case is rejected.

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

    source_exists = ~((~(source_domain & relation)).for_all(tuple[S]))
    forall_target_exists_source = target_domain.satisfies(source_exists).for_all(tuple[T])
    exists_source_forall_target = ~((~(source_domain & target_domain.satisfies(relation).for_all(tuple[T]))).for_all(tuple[S]))

    static_assert(forall_target_exists_source == ConstraintSet.always())
    static_assert(~forall_target_exists_source == ConstraintSet.never())
    static_assert(exists_source_forall_target == ConstraintSet.never())

    source_has_no_witness = (~(source_domain & relation)).for_all(tuple[S])
    target_counterexample = ~((~(target_domain & source_has_no_witness)).for_all(tuple[T]))
    static_assert(target_counterexample == ConstraintSet.never())
    static_assert(target_counterexample == ~forall_target_exists_source)

    int_only = s_int & t_int
    int_source_exists = ~((~(source_domain & int_only)).for_all(tuple[S]))
    missing_str = target_domain.satisfies(int_source_exists).for_all(tuple[T])
    static_assert(missing_str == ConstraintSet.never())
```
