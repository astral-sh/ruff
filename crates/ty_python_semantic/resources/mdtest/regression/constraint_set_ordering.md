# Constraint set ordering

This file verifies that constraint set solutions are deterministic, even in the presence of
different variable orderings in their underlying BDDs.

The current implementation is _stable_ in the sense that multiple runs of `ty` against the same
source will produce the same result. But there are still some lingering places where the output
depends on the particular BDD variable ordering that is chosen.

The diagnostic expectations show the output that is produced by the default stable ordering, so that
if you are not explicitly testing constraint set stability, mdtests should pass. We also include
TODO comments showing the other potential outputs that might be produced, under different variable
orderings. (`ConstraintSet.solutions_for` exposes each explicit per-typevar solution;
`ConstraintSet.solutions` additionally preserves path and binding order. This keeps duplicate and
`Never` solutions visible when they would otherwise disappear as paths are unioned.)

To test this, you can set the `TY_CONSTRAINT_SET_ORDER` environment variable to either `reverse` or
to an integer. This lets you choose a different permutation for each run of `ty`. You can also use
the `wobbling-ty-constraint-order` agent skill to automate this process.

```toml
[environment]
python-version = "3.13"
```

## Solution binding order follows constraint source order

The order of bindings within a path must follow the first constraint that introduced each typevar.
Reversing either the typevar declaration order or the constraint source order exercises both sides
of this requirement.

```py
from ty_extensions._internal import ConstraintSet

def bindings_tuv[T, U, V]() -> None:
    # (T = int) ∧ (U = str) ∧ (V = bytes)
    constraints = ConstraintSet.range(int, T, int) & ConstraintSet.range(str, U, str) & ConstraintSet.range(bytes, V, bytes)
    # revealed: tuple[Solution[T=int, U=str, V=bytes]]
    reveal_type(constraints.solutions(inferable=tuple[T, U, V]))

def bindings_vtu[V, T, U]() -> None:
    # (T = int) ∧ (U = str) ∧ (V = bytes)
    constraints = ConstraintSet.range(int, T, int) & ConstraintSet.range(str, U, str) & ConstraintSet.range(bytes, V, bytes)
    # revealed: tuple[Solution[T=int, U=str, V=bytes]]
    reveal_type(constraints.solutions(inferable=tuple[T, U, V]))

def bindings_reverse_source[T, U, V]() -> None:
    # (V = bytes) ∧ (U = str) ∧ (T = int)
    constraints = ConstraintSet.range(bytes, V, bytes) & ConstraintSet.range(str, U, str) & ConstraintSet.range(int, T, int)
    # revealed: tuple[Solution[V=bytes, U=str, T=int]]
    reveal_type(constraints.solutions(inferable=tuple[T, U, V]))
```

## Nested transitive constraints and an unrelated alternative

In `((T ≤ list[U]) ∧ (U ≤ int) ∧ (list[int] ≤ T)) | (bytes ≤ V)`, the two sides of the union are
completely independent: the solutions for `T` and `U` should not influence the solutions for `V`,
and vice versa. Because we combine them with union, we are allowed to _either_ find a solution for
`T` and `U`, _or_ find a solution for `V`. We are not _obligated_ to find a solution for all three.

```py
from typing import Never
from ty_extensions._internal import ConstraintSet

def nested_transitive[T, U, V]() -> None:
    # ((T ≤ list[U]) ∧ (U ≤ int) ∧ (list[int] ≤ T)) | (bytes ≤ V)
    constraints = (
        ConstraintSet.range(Never, T, list[U]) & ConstraintSet.range(Never, U, int) & ConstraintSet.range(list[int], T, object)
    ) | ConstraintSet.range(bytes, V, object)

    # TODO: sometimes: revealed tuple[Solution[T=list[int]], Solution[T=Never], Solution[]]
    # TODO: sometimes: revealed tuple[Solution[T=list[int]], Solution[T=list[int]], Solution[]]
    # TODO: sometimes: revealed tuple[Solution[T=list[int]], Solution[], Solution[]]
    # revealed: tuple[Solution[T=list[int]], Solution[]]
    reveal_type(constraints.solutions_for(T, inferable=tuple[T, U, V]))

    # TODO: sometimes: revealed tuple[Solution[U=int], Solution[U=Never], Solution[]]
    # TODO: sometimes: revealed tuple[Solution[U=int], Solution[], Solution[]]
    # revealed: tuple[Solution[U=int], Solution[]]
    reveal_type(constraints.solutions_for(U, inferable=tuple[T, U, V]))

    # TODO: sometimes: revealed tuple[Solution[], Solution[V=bytes], Solution[V=bytes]]
    # revealed: tuple[Solution[], Solution[V=bytes]]
    reveal_type(constraints.solutions_for(V, inferable=tuple[T, U, V]))

    # TODO: sometimes: revealed tuple[Solution[T=list[int], U=int], Solution[T=Never, V=bytes], Solution[V=bytes]]
    # TODO: sometimes: revealed tuple[Solution[T=list[int], U=int], Solution[T=list[int], V=bytes], Solution[V=bytes]]
    # TODO: sometimes: revealed tuple[Solution[T=list[int], U=int], Solution[U=Never, V=bytes], Solution[V=bytes]]
    # revealed: tuple[Solution[T=list[int], U=int], Solution[V=bytes]]
    reveal_type(constraints.solutions(inferable=tuple[T, U, V]))
```

## Negated alternatives do not infer positive evidence

In `¬((T ≤ int) ∨ (T ≤ str)) | (bytes ≤ U)`, the lhs of the union is a negation, and should not
place any positive restriction on `T`. Like above, we are not obligated to produce a solution that
includes both sides of the union, so any solution that includes `bytes ≤ U` should not include a
solution for `T`.

```py
from typing import Never
from ty_extensions._internal import ConstraintSet

def negated_alternative[T, U]() -> None:
    # ¬((T ≤ int) ∨ (T ≤ str)) | (bytes ≤ U)
    constraints = ~(ConstraintSet.range(Never, T, int) | ConstraintSet.range(Never, T, str)) | ConstraintSet.range(
        bytes, U, object
    )

    # TODO: sometimes: revealed tuple[Solution[], Solution[T=Never], Solution[]]
    # revealed: tuple[Solution[], Solution[]]
    reveal_type(constraints.solutions_for(T, inferable=tuple[T, U]))

    # TODO: sometimes: revealed tuple[Solution[], Solution[U=bytes], Solution[U=bytes]]
    # revealed: tuple[Solution[], Solution[U=bytes]]
    reveal_type(constraints.solutions_for(U, inferable=tuple[T, U]))

    # TODO: sometimes: revealed tuple[Solution[], Solution[T=Never, U=bytes], Solution[U=bytes]]
    # revealed: tuple[Solution[], Solution[U=bytes]]
    reveal_type(constraints.solutions(inferable=tuple[T, U]))
```

## Derived solution element order

Constructing the constraints in the opposite source order makes the derived union observable. Its
elements should not be reordered merely because the TDD-variable order changes.

```py
from typing import Never
from ty_extensions._internal import ConstraintSet

def derived_solution[U, T]() -> None:
    # (U ≤ int) ∧ (int ≤ T) ∧ ((T ≤ int) | (T ≤ str))
    constraints = (
        ConstraintSet.range(Never, U, int)
        & ConstraintSet.range(int, T, object)
        & (ConstraintSet.range(Never, T, int) | ConstraintSet.range(Never, T, str))
    )

    # TODO: The derived relationship should not leave an inferable `U` in the solution for `T`.
    # TODO: revealed: tuple[Solution[T=int]]
    # TODO: sometimes: revealed tuple[Solution[T=int | U@derived_solution]]
    # revealed: tuple[Solution[T=U@derived_solution | int]]
    reveal_type(constraints.solutions_for(T, inferable=tuple[T, U]))

    # TODO: The derived relationship should not leave an inferable `T` in the solution for `U`.
    # TODO: revealed: tuple[Solution[U=int]]
    # revealed: tuple[Solution[U=Never]]
    reveal_type(constraints.solutions_for(U, inferable=tuple[T, U]))
```

## Bare-typevar orientation and tied source order

`S ≤ T` can be represented as a constraint on either typevar, and `S ≤ T ≤ U` can be either one
range or two linked constraints. Logical equivalence and solution-element order must remain stable
in both declaration orders.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

def orientation_st[S, T]() -> None:
    lower = ConstraintSet.range(Never, S, T)
    upper = ConstraintSet.range(S, T, object)
    # TODO: sometimes: error [static-assert-error] "Static assertion error: argument evaluates to `False`"
    static_assert(lower == upper)

    equality_st = ConstraintSet.range(T, S, T)
    equality_ts = ConstraintSet.range(S, T, S)
    static_assert(equality_st == equality_ts)

def orientation_ts[T, S]() -> None:
    lower = ConstraintSet.range(Never, S, T)
    upper = ConstraintSet.range(S, T, object)
    # TODO: sometimes: error [static-assert-error] "Static assertion error: argument evaluates to `False`"
    static_assert(lower == upper)

    equality_st = ConstraintSet.range(T, S, T)
    equality_ts = ConstraintSet.range(S, T, S)
    static_assert(equality_st == equality_ts)

def chain_stu[S, T, U]() -> None:
    chain = ConstraintSet.range(S, T, U)
    linked = ConstraintSet.range(Never, S, T) & ConstraintSet.range(Never, T, U)
    # TODO: sometimes: error [static-assert-error] "Static assertion error: argument evaluates to `False`"
    static_assert(chain == linked)

    constraints = chain & ConstraintSet.range(int, S, object) & ConstraintSet.range(Never, U, int)
    # TODO: inferable typevars should not remain in these concrete solutions.
    # TODO: sometimes: revealed tuple[Solution[S=int | U@chain_stu | T@chain_stu]]
    # revealed: tuple[Solution[S=T@chain_stu | int | U@chain_stu]]
    reveal_type(constraints.solutions_for(S, inferable=tuple[S, T, U]))
    # revealed: tuple[Solution[T=S@chain_stu | int | U@chain_stu]]
    reveal_type(constraints.solutions_for(T, inferable=tuple[S, T, U]))
    # revealed: tuple[Solution[U=T@chain_stu | S@chain_stu | int]]
    reveal_type(constraints.solutions_for(U, inferable=tuple[S, T, U]))

def chain_uts[U, T, S]() -> None:
    chain = ConstraintSet.range(S, T, U)
    linked = ConstraintSet.range(Never, S, T) & ConstraintSet.range(Never, T, U)
    # TODO: sometimes: error [static-assert-error] "Static assertion error: argument evaluates to `False`"
    static_assert(chain == linked)

    constraints = chain & ConstraintSet.range(int, S, object) & ConstraintSet.range(Never, U, int)
    # TODO: inferable typevars should not remain in these concrete solutions.
    # TODO: sometimes: revealed tuple[Solution[S=int | U@chain_uts | T@chain_uts]]
    # revealed: tuple[Solution[S=T@chain_uts | int | U@chain_uts]]
    reveal_type(constraints.solutions_for(S, inferable=tuple[S, T, U]))
    # revealed: tuple[Solution[T=S@chain_uts | int | U@chain_uts]]
    reveal_type(constraints.solutions_for(T, inferable=tuple[S, T, U]))
    # revealed: tuple[Solution[U=T@chain_uts | S@chain_uts | int]]
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
    # TODO: We should not include a solution for non-inferable U.
    # TODO: sometimes: revealed tuple[Solution[T=list[int], U=int], Solution[T=Never, V=bytes], Solution[V=bytes]]
    # TODO: sometimes: revealed tuple[Solution[T=list[int], U=int], Solution[T=list[int], V=bytes], Solution[V=bytes]]
    # revealed: tuple[Solution[T=list[int], U=int], Solution[V=bytes]]
    reveal_type(constraints.solutions(inferable=tuple[T, V]))
    # TODO: sometimes: revealed tuple[Solution[T=list[int]], Solution[T=Never], Solution[]]
    # TODO: sometimes: revealed tuple[Solution[T=list[int]], Solution[T=list[int]], Solution[]]
    # revealed: tuple[Solution[T=list[int]], Solution[]]
    reveal_type(constraints.solutions_for(T, inferable=tuple[T, V]))
    # TODO: sometimes: revealed tuple[Solution[], Solution[V=bytes], Solution[V=bytes]]
    # revealed: tuple[Solution[], Solution[V=bytes]]
    reveal_type(constraints.solutions_for(V, inferable=tuple[T, V]))

    quantified = constraints.for_all(tuple[T, U])
    expected = ConstraintSet.range(bytes, V, object)
    static_assert(quantified == expected)
    # revealed: tuple[Solution[V=bytes]]
    reveal_type(quantified.solutions_for(V, inferable=tuple[V]))

def noninferable_negated[T, U]() -> None:
    constraints = ~(ConstraintSet.range(Never, T, int) | ConstraintSet.range(Never, T, str)) | ConstraintSet.range(
        bytes, U, object
    )

    quantified = constraints.for_all(tuple[T])
    expected = ConstraintSet.range(bytes, U, object)
    static_assert(quantified == expected)
    # revealed: tuple[Solution[U=bytes]]
    reveal_type(quantified.solutions_for(U, inferable=tuple[U]))
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

# revealed: P & Q
reveal_type(infer_from_callbacks(accepts_p, accepts_q))
# revealed: Q & P
reveal_type(infer_from_callbacks(accepts_q, accepts_p))
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
    # TODO: sometimes: no error
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
    # TODO: sometimes: revealed object
    # revealed: int
    reveal_type(get_value(value))
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

# error: [invalid-assignment]
invalid: Array = Concrete[int]()
```

## High-fanout sequents and inferred-union truncation

The cross-product between the twelve lower- and twelve upper-bound relationships exhausts the shared
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
    # TODO: sometimes: revealed tuple[Solution[P=L0@high_fanout | Literal[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11] | L1@high_fanout | L2@high_fanout | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[P=L0@high_fanout | Literal[0, 3, 4, 5, 6, 7, 8, 9, 10, 11] | L1@high_fanout | L2@high_fanout | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[P=L0@high_fanout | Literal[0, 1, 4, 5, 6, 7, 8, 9, 10, 11] | L1@high_fanout | L2@high_fanout | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[P=L0@high_fanout | Literal[0, 1, 2, 5, 6, 7, 8, 9, 10, 11] | L1@high_fanout | L2@high_fanout | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[P=L0@high_fanout | Literal[0, 1, 2, 3, 6, 7, 8, 9, 10, 11] | L1@high_fanout | L2@high_fanout | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[P=L0@high_fanout | Literal[0, 1, 2, 3, 4, 5, 6, 9, 10, 11] | L1@high_fanout | L2@high_fanout | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout]]
    # revealed: tuple[Solution[P=L0@high_fanout | L1@high_fanout | L2@high_fanout | Literal[2, 3, 4, 5, 6, 7, 8, 9, 10, 11] | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout]]
    reveal_type(pivot)

    # TODO: sometimes: revealed tuple[Solution[R11=P@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[R11=L0@high_fanout | L2@high_fanout | Literal[2, 3, 4, 5, 6, 7, 8, 9, 10, 11] | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout | P@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[R11=L0@high_fanout | Literal[0, 3, 4, 5, 6, 7, 8, 9, 10, 11] | L2@high_fanout | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout | P@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[R11=L0@high_fanout | Literal[0, 1, 4, 5, 6, 7, 8, 9, 10, 11] | L1@high_fanout | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout | P@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[R11=L0@high_fanout | Literal[0, 1, 5, 6, 7, 8, 9, 10, 11] | L1@high_fanout | L2@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout | P@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[R11=L0@high_fanout | Literal[0, 1, 2, 3, 6, 7, 8, 9, 10, 11] | L1@high_fanout | L2@high_fanout | L3@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout | P@high_fanout]]
    # TODO: sometimes: revealed tuple[Solution[R11=L0@high_fanout | Literal[0, 1, 2, 3, 4, 5, 6, 9, 10, 11] | L1@high_fanout | L2@high_fanout | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout | P@high_fanout]]
    # revealed: tuple[Solution[R11=L1@high_fanout | L2@high_fanout | Literal[2, 3, 4, 5, 6, 7, 8, 9, 10, 11] | L3@high_fanout | L4@high_fanout | L5@high_fanout | L6@high_fanout | L7@high_fanout | L8@high_fanout | L9@high_fanout | L10@high_fanout | L11@high_fanout | P@high_fanout]]
    reveal_type(result)

    impossible = constraints & ConstraintSet.range(Never, R11, Literal[0])
    # TODO: sometimes: error [static-assert-error] "Static assertion error: argument evaluates to `False`"
    static_assert(not impossible.satisfied_by_all_typevars(inferable=inferable))
```
