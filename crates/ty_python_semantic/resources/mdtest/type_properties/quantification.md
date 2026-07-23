# Quantification

Quantification removes typevars from a constraint set, returning a new equivalent constraint set
that only references the remaining typevars. With existential quantification (`exists`), the result
holds when there is _at least one_ valid assignment of the removed variables that satisfies the
quantified expression. With universal quantification (`for_all`), the result holds when _every_
valid assignment satisfies the quantified expression.

This file contains several baseline test cases that validate our implementation of quantification.

| Case | Formula                          | Expected result                             |
| ---- | -------------------------------- | ------------------------------------------- |
| C0   | `∃X. X = int ∧ A ≤ Invariant[X]` | Equivalent to `A ≤ Invariant[int]`          |
| E1   | `∃X. U ≤ X ∧ X = V`              | Equivalent to `U ≤ V`                       |
| E2   | `∃X. A ≤ Invariant[X] ∧ X ≤ B`   | `A` and `B` must admit a common `X`         |
| E3   | `∃X. A ≤ X ∧ Invariant[X] ≤ B`   | `A` and `B` must admit a common `X`         |
| E4   | `∃X. C₁(X, Y) ∧ C₂(X, Z)`        | Solutions for `Y` and `Z` remain correlated |
| E5   | `∃X ∈ {int, str}. C(X, Y, Z)`    | Solutions remain paired with each choice    |
| E6   | `∀Y ∈ Dᵧ. ∃X ∈ Dₓ. R(X, Y)`      | `X` may depend on the choice of `Y`         |

```toml
[environment]
python-version = "3.13"
```

## C0: grounded invariant

In `∃X. X = int ∧ A ≤ Invariant[X]`, every assignment of `X` is _valid_ (i.e., satisfies the
implicit upper bound of `object`), but the only _satisfying_ assignment is `X = int`. That means the
result should be equivalent to `A ≤ Invariant[int]`.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Invariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def set(self, value: T) -> None: ...

def grounded[X, A]() -> None:
    # ∃X. X = int ∧ A ≤ Invariant[X]
    body = ConstraintSet.range(int, X, int) & ConstraintSet.range(Never, A, Invariant[X])
    quantified = body.exists(tuple[X])

    # TODO: revealed: tuple[Solution[X=int, A=list[int]]]
    # revealed: tuple[Solution[X=int, A=Never]]
    reveal_type(body.solutions(inferable=tuple[X, A]))
    # TODO: revealed: tuple[Solution[A=list[int]]]
    # revealed: tuple[Solution[A=Never]]
    reveal_type(quantified.solutions(inferable=tuple[A]))

    # A ≤ Invariant[int]
    expected = ConstraintSet.range(Never, A, Invariant[int])
    static_assert(quantified == expected)
    static_assert(~quantified == ~expected)
```

## E1: relational bridge

There is an `X` satisfying `U ≤ X ∧ X = V` exactly when `U ≤ V`.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def relational_bridge[X, U, V]() -> None:
    # ∃X. U ≤ X ∧ X = V
    body = ConstraintSet.range(Never, U, X) & ConstraintSet.range(V, X, V)
    quantified = body.exists(tuple[X])

    # TODO: revealed: tuple[Solution[V=object, U=object]]
    # revealed: tuple[Solution[V=U@relational_bridge, U=Never]]
    reveal_type(quantified.solutions(inferable=tuple[U, V]))

    # U ≤ V
    expected = ConstraintSet.range(Never, U, V)
    static_assert(quantified == expected)
    static_assert(~quantified == ~expected)
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
    # ∃X. A ≤ Invariant[X] ∧ X ≤ B
    body = ConstraintSet.range(Never, A, Invariant[X]) & ConstraintSet.range(Never, X, B)
    quantified = body.exists(tuple[X])

    # TODO: revealed: tuple[Solution[A=Invariant[object], B=object, X=object]]
    # revealed: tuple[Solution[A=Never, B=X@inverse_image, X=Never]]
    reveal_type(body.solutions(inferable=tuple[X, A, B]))
    # TODO: revealed: tuple[Solution[A=Invariant[object], B=object]]
    # revealed: tuple[()]
    reveal_type(quantified.solutions(inferable=tuple[A, B]))

    # Invariant[str] ≤ A ∧ B ≤ int
    invalid = ConstraintSet.range(Invariant[str], A, object) & ConstraintSet.range(Never, B, int)
    # revealed: None
    reveal_type((body & invalid).solutions(inferable=tuple[X, A, B]))
    # TODO: revealed: None
    # revealed: tuple[Solution[A=Invariant[str], B=Never]]
    reveal_type((quantified & invalid).solutions(inferable=tuple[A, B]))

    static_assert(not (quantified & invalid))
    # TODO: no error
    # error: [static-assert-error]
    static_assert((~quantified & invalid) == invalid)
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
    # ∃X. A ≤ X ∧ Invariant[X] ≤ B
    body = ConstraintSet.range(A, X, object) & ConstraintSet.range(Invariant[X], B, object)
    quantified = body.exists(tuple[X])

    # Each solution for A and B depends on the compatible choice of X.
    # TODO: revealed: tuple[Solution[X=object, A=object, B=Invariant[object]]]
    # revealed: tuple[Solution[X=A@witness_sensitive, A=X@witness_sensitive, B=Invariant[X@witness_sensitive]]]
    reveal_type(body.solutions(inferable=tuple[X, A, B]))
    # TODO: revealed: tuple[Solution[A=object, B=Invariant[object]]]
    # revealed: tuple[()]
    reveal_type(quantified.solutions(inferable=tuple[A, B]))

    # int ≤ A ∧ B ≤ Invariant[str]
    invalid = ConstraintSet.range(int, A, object) & ConstraintSet.range(Never, B, Invariant[str])
    # revealed: None
    reveal_type((body & invalid).solutions(inferable=tuple[X, A, B]))
    # TODO: revealed: None
    # revealed: tuple[Solution[A=int, B=Never]]
    reveal_type((quantified & invalid).solutions(inferable=tuple[A, B]))

    static_assert(not (quantified & invalid))
    # TODO: no error
    # error: [static-assert-error]
    static_assert((~quantified & invalid) == invalid)
```

## E4: correlated visible outputs

`C₁` relates `X` to `Y`, while `C₂` relates `X` to `Z`. Both constraints must hold for the same
choice of `X`. The two valid solution families are `(Y = int, Z = Invariant[int])` and
`(Y = str, Z = Invariant[str])`; the cross-pairing `(Y = int, Z = Invariant[str])` is invalid.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Invariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def set(self, value: T) -> None: ...

def correlated_outputs[X, Y, Z]() -> None:
    # C₁(X, Y) = (X = int ∧ Y = int) ∨ (X = str ∧ Y = str)
    c1_int = ConstraintSet.range(int, X, int) & ConstraintSet.range(int, Y, int)
    c1_str = ConstraintSet.range(str, X, str) & ConstraintSet.range(str, Y, str)
    c1 = c1_int | c1_str

    # C₂(X, Z) = (Z = Invariant[X])
    c2 = ConstraintSet.range(Invariant[X], Z, Invariant[X])

    # ∃X. C₁(X, Y) ∧ C₂(X, Z)
    body = c1 & c2
    quantified = body.exists(tuple[X])

    # TODO: revealed: tuple[Solution[X=int, Y=int, Z=Invariant[int]], Solution[X=str, Y=str, Z=Invariant[str]]]
    # revealed: tuple[Solution[X=int | Y@correlated_outputs, Z=Invariant[X@correlated_outputs] | Invariant[int], Y=int], Solution[X=str | Y@correlated_outputs, Z=Invariant[X@correlated_outputs] | Invariant[str], Y=str]]
    reveal_type(body.solutions(inferable=tuple[X, Y, Z]))
    # revealed: tuple[Solution[Z=Invariant[int], Y=int], Solution[Z=Invariant[str], Y=str]]
    reveal_type(quantified.solutions(inferable=tuple[Y, Z]))

    # (Y = int ∧ Z = Invariant[int]) ∨ (Y = str ∧ Z = Invariant[str])
    expected_int = ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[int], Z, Invariant[int])
    expected_str = ConstraintSet.range(str, Y, str) & ConstraintSet.range(Invariant[str], Z, Invariant[str])
    expected = expected_int | expected_str
    static_assert(quantified == expected)
    static_assert(~quantified == ~expected)

    # (Y = int ∧ Z = Invariant[str])
    invalid_cross = ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[str], Z, Invariant[str])
    static_assert(not (quantified & invalid_cross))
    # revealed: None
    reveal_type((quantified & invalid_cross).solutions(inferable=tuple[Y, Z]))
```

## E5: finite domain

The declaration of `X` constrains it to be either `int` or `str`. Each valid choice gives a separate
solution family. After `X` is quantified, `Y` and `Z` must remain correlated in each solution, and
specializations outside the declared domain must be rejected.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

class Invariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def set(self, value: T) -> None: ...

def finite_domain[X: (int, str), Y, Z]() -> None:
    # ∃X ∈ {int, str}. C(X, Y, Z)
    # C(X, Y, Z) = (Y = X) ∧ (Z = Invariant[X])
    body = ConstraintSet.range(X, Y, X) & ConstraintSet.range(Invariant[X], Z, Invariant[X])
    quantified = body.exists(tuple[X])

    # TODO: revealed: tuple[Solution[X=int, Y=int, Z=Invariant[int]], Solution[X=str, Y=str, Z=Invariant[str]]]
    # revealed: tuple[Solution[X=Y@finite_domain, Y=X@finite_domain, Z=Invariant[X@finite_domain] | Invariant[Y@finite_domain]]]
    reveal_type(body.solutions(inferable=tuple[X, Y, Z]))
    # TODO: revealed: tuple[Solution[Y=int, Z=Invariant[int]], Solution[Y=str, Z=Invariant[str]]]
    # revealed: tuple[Solution[Z=Invariant[Y@finite_domain]]]
    reveal_type(quantified.solutions(inferable=tuple[Y, Z]))

    # (Y = int ∧ Z = Invariant[int]) ∨ (Y = str ∧ Z = Invariant[str])
    expected_int = ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[int], Z, Invariant[int])
    expected_str = ConstraintSet.range(str, Y, str) & ConstraintSet.range(Invariant[str], Z, Invariant[str])
    expected = expected_int | expected_str
    # TODO: no error
    # error: [static-assert-error]
    static_assert(quantified == expected)
    # TODO: no error
    # error: [static-assert-error]
    static_assert(~quantified == ~expected)

    # (Y = int ∧ Z = Invariant[str])
    invalid_cross = ConstraintSet.range(int, Y, int) & ConstraintSet.range(Invariant[str], Z, Invariant[str])
    static_assert(not (quantified & invalid_cross))
    # revealed: None
    reveal_type((quantified & invalid_cross).solutions(inferable=tuple[Y, Z]))

    # (Y = bytes ∧ Z = Invariant[bytes])
    invalid_domain = ConstraintSet.range(bytes, Y, bytes) & ConstraintSet.range(Invariant[bytes], Z, Invariant[bytes])
    static_assert(not (quantified & invalid_domain))
    # TODO: revealed: None
    # revealed: tuple[Solution[Z=Invariant[Y@finite_domain] | Invariant[bytes], Y=bytes]]
    reveal_type((quantified & invalid_domain).solutions(inferable=tuple[Y, Z]))
```

## E6: alternation and negative polarity

The declarations of `X` and `Y` constrain both variables to `int` or `str`. For every valid choice
of `Y`, there is a matching choice of `X`. Reversing the quantifiers would require one choice of `X`
to work for every `Y` and is therefore false. Negating the relation asks whether there is a `Y` with
no matching `X`; an `int`-only relation shows that a missing `str` case is rejected.

```py
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def alternation[X: (int, str), Y: (int, str)]() -> None:
    # R(X, Y) = (X = int ∧ Y = int) ∨ (X = str ∧ Y = str)
    x_int = ConstraintSet.range(int, X, int)
    x_str = ConstraintSet.range(str, X, str)
    y_int = ConstraintSet.range(int, Y, int)
    y_str = ConstraintSet.range(str, Y, str)
    relation = (x_int & y_int) | (x_str & y_str)

    # ∀Y. ∃X. R(X, Y)
    forall_y_exists_x = relation.exists(tuple[X]).for_all(tuple[Y])
    # TODO: no error
    # error: [static-assert-error]
    static_assert(forall_y_exists_x)
    # TODO: no error
    # error: [static-assert-error]
    static_assert(not ~forall_y_exists_x)

    # ∃X. ∀Y. R(X, Y)
    exists_x_forall_y = relation.for_all(tuple[Y]).exists(tuple[X])
    static_assert(not exists_x_forall_y)

    # ∃Y. ∀X. ¬R(X, Y)
    counterexample = (~relation).for_all(tuple[X]).exists(tuple[Y])
    # TODO: no error
    # error: [static-assert-error]
    static_assert(not counterexample)
    static_assert(counterexample == ~forall_y_exists_x)

    int_only = x_int & y_int
    missing_str = int_only.exists(tuple[X]).for_all(tuple[Y])
    static_assert(not missing_str)
```
