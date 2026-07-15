# Constraint-set ordering

These regressions exercise solution extraction while changing the builder-local constraint and
typevar ordering with `TY_CONSTRAINT_SET_ORDER`. `ConstraintSet.solutions_for` exposes each explicit
per-path solution as a `TypeForm`, including duplicate and `Never` solutions that would otherwise
disappear when paths are unioned.

```toml
[environment]
python-version = "3.13"
```

## Nested transitive constraints and an unrelated alternative

The `bytes ≤ V` alternative should not acquire `T` or `U` solutions from the nested transitive
branch.

```py
from typing import Never
from ty_extensions._internal import ConstraintSet

def nested_transitive[T, U, V]() -> None:
    constraints = (
        ConstraintSet.range(Never, T, list[U]) & ConstraintSet.range(Never, U, int) & ConstraintSet.range(list[int], T, object)
    ) | ConstraintSet.range(bytes, V, object)

    reveal_type(constraints.solutions_for(T, inferable=tuple[T, U, V]))  # revealed: tuple[TypeForm[list[int]]]
    reveal_type(constraints.solutions_for(U, inferable=tuple[T, U, V]))  # revealed: tuple[TypeForm[int]]
    reveal_type(constraints.solutions_for(V, inferable=tuple[T, U, V]))  # revealed: tuple[TypeForm[bytes]]
```

## Negated alternatives do not infer positive evidence

The negated branch places no positive restriction on `T`. In particular, satisfying the unrelated
`bytes ≤ U` branch should not cause a positive `T` solution to appear.

```py
from typing import Never
from ty_extensions._internal import ConstraintSet

def negated_alternative[T, U]() -> None:
    constraints = ~(ConstraintSet.range(Never, T, int) | ConstraintSet.range(Never, T, str)) | ConstraintSet.range(
        bytes, U, object
    )

    reveal_type(constraints.solutions_for(T, inferable=tuple[T, U]))  # revealed: tuple[()]
    reveal_type(constraints.solutions_for(U, inferable=tuple[T, U]))  # revealed: tuple[TypeForm[bytes]]
```

## Derived solution element order

Constructing the constraints in the opposite source order makes the derived union observable. Its
elements should not be reordered merely because the TDD-variable order changes.

```py
from typing import Never
from ty_extensions._internal import ConstraintSet

def derived_solution[U, T]() -> None:
    constraints = (
        ConstraintSet.range(Never, U, int)
        & ConstraintSet.range(int, T, object)
        & (ConstraintSet.range(Never, T, int) | ConstraintSet.range(Never, T, str))
    )

    # TODO: The derived relationship should not leave an inferable `U` in the solution for `T`.
    reveal_type(constraints.solutions_for(T, inferable=tuple[T, U]))  # revealed: tuple[TypeForm[U@derived_solution | int]]
    # TODO: revealed: tuple[TypeForm[T@derived_solution & int]]
    reveal_type(constraints.solutions_for(U, inferable=tuple[T, U]))  # revealed: tuple[TypeForm[Never]]
```

## High-fanout sequents and inferred-union truncation

The cross-product between the twelve lower and twelve upper relationships exhausts the shared
sequent fuel budget. The remaining solution, its element order, and the elements retained by
truncated diagnostic display must not depend on which implications were encountered first.

```py
from typing import Literal, Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def high_fanout[
    P,
    L0,
    L1,
    L2,
    L3,
    L4,
    L5,
    L6,
    L7,
    L8,
    L9,
    L10,
    L11,
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
    R9,
    R10,
    R11,
]() -> None:
    lower = (
        ConstraintSet.range(Literal[0], L0, P)
        & ConstraintSet.range(Literal[1], L1, P)
        & ConstraintSet.range(Literal[2], L2, P)
        & ConstraintSet.range(Literal[3], L3, P)
        & ConstraintSet.range(Literal[4], L4, P)
        & ConstraintSet.range(Literal[5], L5, P)
        & ConstraintSet.range(Literal[6], L6, P)
        & ConstraintSet.range(Literal[7], L7, P)
        & ConstraintSet.range(Literal[8], L8, P)
        & ConstraintSet.range(Literal[9], L9, P)
        & ConstraintSet.range(Literal[10], L10, P)
        & ConstraintSet.range(Literal[11], L11, P)
    )
    upper = (
        ConstraintSet.range(Never, P, R0)
        & ConstraintSet.range(Never, P, R1)
        & ConstraintSet.range(Never, P, R2)
        & ConstraintSet.range(Never, P, R3)
        & ConstraintSet.range(Never, P, R4)
        & ConstraintSet.range(Never, P, R5)
        & ConstraintSet.range(Never, P, R6)
        & ConstraintSet.range(Never, P, R7)
        & ConstraintSet.range(Never, P, R8)
        & ConstraintSet.range(Never, P, R9)
        & ConstraintSet.range(Never, P, R10)
        & ConstraintSet.range(Never, P, R11)
    )
    inferable = tuple[
        P,
        L0,
        L1,
        L2,
        L3,
        L4,
        L5,
        L6,
        L7,
        L8,
        L9,
        L10,
        L11,
        R0,
        R1,
        R2,
        R3,
        R4,
        R5,
        R6,
        R7,
        R8,
        R9,
        R10,
        R11,
    ]
    constraints = lower & upper
    pivot = constraints.solutions_for(P, inferable=inferable)
    result = constraints.solutions_for(R11, inferable=inferable)

    # TODO: inferred solutions should not retain the intermediate inferable typevars.
    # revealed: tuple[TypeForm[L0@high_fanout | L1@high_fanout | L2@high_fanout | Literal[2, 3, 4, 5, 6, 7, 8, 9, 10, 11] | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout]]
    reveal_type(pivot)
    # revealed: tuple[TypeForm[L1@high_fanout | L2@high_fanout | Literal[2, 3, 4, 5, 6, 7, 8, 9, 10, 11] | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout | P@high_fanout]]
    reveal_type(result)

    def takes_empty(value: tuple[()]) -> None: ...

    # snapshot: invalid-argument-type
    takes_empty(pivot)
    # snapshot: invalid-argument-type
    takes_empty(result)

    impossible = constraints & ConstraintSet.range(Never, R11, Literal[0])
    static_assert(not impossible.satisfied_by_all_typevars(inferable=inferable))
```

```snapshot
error[invalid-argument-type]: Argument to function `takes_empty` is incorrect
   --> src/mdtest_snippet.py:100:17
    |
100 |     takes_empty(pivot)
    |                 ^^^^^ Expected `tuple[()]`, found `tuple[TypeForm[L0@high_fanout | L1@high_fanout | L2@high_fanout | ... omitted 10 union elements]]`
    |
info: a tuple of length 1 is not assignable to a tuple of length 0
info: Function defined here
  --> src/mdtest_snippet.py:97:9
   |
97 |     def takes_empty(value: tuple[()]) -> None: ...
   |         ^^^^^^^^^^^ ---------------- Parameter declared here
   |


error[invalid-argument-type]: Argument to function `takes_empty` is incorrect
   --> src/mdtest_snippet.py:102:17
    |
102 |     takes_empty(result)
    |                 ^^^^^^ Expected `tuple[()]`, found `tuple[TypeForm[L1@high_fanout | L2@high_fanout | Literal[2, 3, 4, 5, 6, ... omitted 5 literals] | ... omitted 10 union elements]]`
    |
info: a tuple of length 1 is not assignable to a tuple of length 0
info: Function defined here
  --> src/mdtest_snippet.py:97:9
   |
97 |     def takes_empty(value: tuple[()]) -> None: ...
   |         ^^^^^^^^^^^ ---------------- Parameter declared here
   |
```
