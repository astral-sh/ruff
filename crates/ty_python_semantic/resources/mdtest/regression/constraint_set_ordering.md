# Constraint-set ordering

These regressions exercise solution extraction while changing the builder-local constraint and
typevar ordering with `TY_CONSTRAINT_SET_ORDER`. `ConstraintSet.solutions_for` exposes each explicit
per-typevar solution as a `TypeForm`; `ConstraintSet.solutions` additionally preserves path and
binding order. This keeps duplicate and `Never` solutions visible when they would otherwise
disappear as paths are unioned.

```toml
[environment]
python-version = "3.13"
```

## Solution binding order follows constraint source order

The order of bindings within a path must follow the first constraint that introduced each typevar,
not hashes of their Salsa-backed identities. Reversing either the typevar declaration order or the
constraint source order exercises both sides of this requirement.

```py
from ty_extensions._internal import ConstraintSet

def bindings_tuv[T, U, V]() -> None:
    constraints = ConstraintSet.range(int, T, int) & ConstraintSet.range(str, U, str) & ConstraintSet.range(bytes, V, bytes)
    # revealed: tuple[tuple[TypeForm[int], TypeForm[str], TypeForm[bytes]]]
    reveal_type(constraints.solutions(inferable=tuple[T, U, V]))

def bindings_vtu[V, T, U]() -> None:
    constraints = ConstraintSet.range(int, T, int) & ConstraintSet.range(str, U, str) & ConstraintSet.range(bytes, V, bytes)
    # revealed: tuple[tuple[TypeForm[int], TypeForm[str], TypeForm[bytes]]]
    reveal_type(constraints.solutions(inferable=tuple[T, U, V]))

def bindings_reverse_source[T, U, V]() -> None:
    constraints = ConstraintSet.range(bytes, V, bytes) & ConstraintSet.range(str, U, str) & ConstraintSet.range(int, T, int)
    # revealed: tuple[tuple[TypeForm[bytes], TypeForm[str], TypeForm[int]]]
    reveal_type(constraints.solutions(inferable=tuple[T, U, V]))
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
    # revealed: tuple[tuple[TypeForm[list[int]], TypeForm[int]], tuple[TypeForm[bytes]]]
    reveal_type(constraints.solutions(inferable=tuple[T, U, V]))
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
    # revealed: tuple[tuple[()], tuple[TypeForm[bytes]]]
    reveal_type(constraints.solutions(inferable=tuple[T, U]))
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

## Bare-typevar orientation and tied source order

`S ≤ T` can be represented as a constraint on either typevar, and `S ≤ T ≤ U` can be either one
range or two linked constraints. Reorientation combines nodes whose source orders can tie, so
logical equivalence and solution-element order must remain stable in both declaration orders.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def orientation_st[S, T]() -> None:
    lower = ConstraintSet.range(Never, S, T)
    upper = ConstraintSet.range(S, T, object)
    static_assert(lower == upper)

    equality_st = ConstraintSet.range(T, S, T)
    equality_ts = ConstraintSet.range(S, T, S)
    static_assert(equality_st == equality_ts)

def orientation_ts[T, S]() -> None:
    lower = ConstraintSet.range(Never, S, T)
    upper = ConstraintSet.range(S, T, object)
    static_assert(lower == upper)

    equality_st = ConstraintSet.range(T, S, T)
    equality_ts = ConstraintSet.range(S, T, S)
    static_assert(equality_st == equality_ts)

def chain_stu[S, T, U]() -> None:
    chain = ConstraintSet.range(S, T, U)
    linked = ConstraintSet.range(Never, S, T) & ConstraintSet.range(Never, T, U)
    static_assert(chain == linked)

    constraints = chain & ConstraintSet.range(int, S, object) & ConstraintSet.range(Never, U, int)
    # TODO: inferable typevars should not remain in these concrete solutions.
    # revealed: tuple[TypeForm[T@chain_stu | int | U@chain_stu]]
    reveal_type(constraints.solutions_for(S, inferable=tuple[S, T, U]))
    # revealed: tuple[TypeForm[S@chain_stu | int | U@chain_stu]]
    reveal_type(constraints.solutions_for(T, inferable=tuple[S, T, U]))
    # revealed: tuple[TypeForm[T@chain_stu | S@chain_stu | int]]
    reveal_type(constraints.solutions_for(U, inferable=tuple[S, T, U]))

def chain_uts[U, T, S]() -> None:
    chain = ConstraintSet.range(S, T, U)
    linked = ConstraintSet.range(Never, S, T) & ConstraintSet.range(Never, T, U)
    static_assert(chain == linked)

    constraints = chain & ConstraintSet.range(int, S, object) & ConstraintSet.range(Never, U, int)
    # TODO: inferable typevars should not remain in these concrete solutions.
    # revealed: tuple[TypeForm[T@chain_uts | int | U@chain_uts]]
    reveal_type(constraints.solutions_for(S, inferable=tuple[S, T, U]))
    # revealed: tuple[TypeForm[S@chain_uts | int | U@chain_uts]]
    reveal_type(constraints.solutions_for(T, inferable=tuple[S, T, U]))
    # revealed: tuple[TypeForm[T@chain_uts | S@chain_uts | int]]
    reveal_type(constraints.solutions_for(U, inferable=tuple[S, T, U]))
```

## Abstraction and non-inferable typevars

Removing non-inferable typevars rebuilds the TDD with `ite`; irrelevant positive decisions must not
leak onto the surviving paths. Universal abstraction of an alternative must likewise leave only the
unrelated branch.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def noninferable_nested[T, U, V]() -> None:
    constraints = (
        ConstraintSet.range(Never, T, list[U]) & ConstraintSet.range(Never, U, int) & ConstraintSet.range(list[int], T, object)
    ) | ConstraintSet.range(bytes, V, object)

    # `U` is deliberately non-inferable here.
    # revealed: tuple[tuple[TypeForm[list[int]], TypeForm[int]], tuple[TypeForm[bytes]]]
    reveal_type(constraints.solutions(inferable=tuple[T, V]))
    reveal_type(constraints.solutions_for(T, inferable=tuple[T, V]))  # revealed: tuple[TypeForm[list[int]]]
    reveal_type(constraints.solutions_for(V, inferable=tuple[T, V]))  # revealed: tuple[TypeForm[bytes]]

    quantified = constraints.for_all(tuple[T, U])
    expected = ConstraintSet.range(bytes, V, object)
    static_assert(quantified == expected)
    reveal_type(quantified.solutions_for(V, inferable=tuple[V]))  # revealed: tuple[TypeForm[bytes]]

def noninferable_negated[T, U]() -> None:
    constraints = ~(ConstraintSet.range(Never, T, int) | ConstraintSet.range(Never, T, str)) | ConstraintSet.range(
        bytes, U, object
    )

    quantified = constraints.for_all(tuple[T])
    expected = ConstraintSet.range(bytes, U, object)
    static_assert(quantified == expected)
    reveal_type(quantified.solutions_for(U, inferable=tuple[U]))  # revealed: tuple[TypeForm[bytes]]
```

## Call-site upper bounds preserve intersection order

Upper bounds inferred from contravariant callable parameters are intersected in call-site source
order. This exercises the direct `UpperBound` insertion path separately from sequent-derived bounds.

```py
from typing import Callable, Protocol, TypeVar

class P(Protocol):
    def p(self) -> None: ...

class Q(Protocol):
    def q(self) -> None: ...

T = TypeVar("T")

def accepts_p(value: P) -> None: ...
def accepts_q(value: Q) -> None: ...
def infer_from_callbacks(first: Callable[[T], None], second: Callable[[T], None]) -> T:
    raise NotImplementedError

reveal_type(infer_from_callbacks(accepts_p, accepts_q))  # revealed: P & Q
reveal_type(infer_from_callbacks(accepts_q, accepts_p))  # revealed: Q & P
```

## Generic-callable and protocol relation constraints

Relations can introduce fresh typevars and nested invariant constraints before those typevars are
quantified away. A `TypedDict` union additionally exercises common-constraint probing and the
fallback protocol-inference path; neither should depend on TDD order.

```py
from typing import Callable, Literal, Protocol, TypeVar, TypedDict
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet, TypeOf

def listify[T](value: T) -> list[T]:
    return [value]

def invariant_callable[U, V]() -> None:
    constraints = ConstraintSet.range(bool, U, int) & ConstraintSet.range(int, V, int)
    # TODO: no error. Existential reduction of the callable's fresh typevar is currently lossy.
    # error: [static-assert-error]
    static_assert(constraints.implies_subtype_of(TypeOf[listify], Callable[[U], list[V]]))

ConstrainedValue = TypeVar("ConstrainedValue", int, object, covariant=True)

class GetValue(Protocol[ConstrainedValue]):
    def __getitem__(self, key: Literal["value"], /) -> ConstrainedValue: ...

class ValueA(TypedDict):
    value: int

class ValueB(TypedDict):
    value: int

def get_value(value: GetValue[ConstrainedValue]) -> ConstrainedValue:
    raise NotImplementedError

def typed_dict_union(value: ValueA | ValueB) -> None:
    reveal_type(get_value(value))  # revealed: int
```

## Recursive derived relations remain cycle-safe

Derived constraints can recursively invoke relation checking. The coinductive owned-set cycle
boundary must continue to terminate without accepting an incompatible non-recursive member when
ordering changes.

```py
from __future__ import annotations
from typing import Protocol, cast

class Array(Protocol):
    def __abs__(self) -> Array: ...
    def __pos__(self) -> Array: ...
    def marker(self) -> int: ...

class Concrete[T]:
    def __abs__[S](self: S) -> S:
        return self

    def __pos__[S](self: S) -> S:
        return self

    def marker(self) -> str:
        return ""

def convert[T](value: Concrete[T]) -> Array:
    return cast(Array, value)

invalid: Array = Concrete[int]()  # error: [invalid-assignment]
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
