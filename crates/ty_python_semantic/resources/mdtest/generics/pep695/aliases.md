# Generic type aliases: PEP 695 syntax

```toml
[environment]
python-version = "3.13"
```

## Defining a generic alias

At its simplest, to define a type alias using PEP 695 syntax, you add a list of `TypeVar`s,
`ParamSpec`s or `TypeVarTuple`s after the alias name.

```py
from ty_extensions import generic_context

type SingleTypevar[T] = ...
type MultipleTypevars[T, S] = ...
type SingleParamSpec[**P] = ...
type TypeVarAndParamSpec[T, **P] = ...
type SingleTypeVarTuple[*Ts] = ...
type TypeVarAndTypeVarTuple[T, *Ts] = ...

# revealed: tuple[T@SingleTypevar]
reveal_type(generic_context(SingleTypevar))
# revealed: tuple[T@MultipleTypevars, S@MultipleTypevars]
reveal_type(generic_context(MultipleTypevars))

# TODO: support `ParamSpec`/`TypeVarTuple` properly
# (these should include the `ParamSpec`s and `TypeVarTuple`s in their generic contexts)
reveal_type(generic_context(SingleParamSpec))  # revealed: tuple[()]
reveal_type(generic_context(TypeVarAndParamSpec))  # revealed: tuple[T@TypeVarAndParamSpec]
reveal_type(generic_context(SingleTypeVarTuple))  # revealed: tuple[()]
reveal_type(generic_context(TypeVarAndTypeVarTuple))  # revealed: tuple[T@TypeVarAndTypeVarTuple]
```

You cannot use the same typevar more than once.

```py
# error: [invalid-syntax] "duplicate type parameter"
type RepeatedTypevar[T, T] = ...
```

## Specializing type aliases explicitly

The type parameter can be specified explicitly:

```py
from typing import Literal

type C[T] = T

def _(a: C[int], b: C[Literal[5]]):
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: Literal[5]
```

The specialization must match the generic types:

```py
# error: [too-many-positional-arguments] "Too many positional arguments: expected 1, got 2"
reveal_type(C[int, int])  # revealed: Unknown
```

And non-generic types cannot be specialized:

```py
type B = ...

# error: [non-subscriptable] "Cannot subscript non-generic type alias"
reveal_type(B[int])  # revealed: Unknown

# error: [non-subscriptable] "Cannot subscript non-generic type alias"
def _(b: B[int]): ...
```

If the type variable has an upper bound, the specialized type must satisfy that bound:

```py
type Bounded[T: int] = ...
type BoundedByUnion[T: int | str] = ...

class IntSubclass(int): ...

reveal_type(Bounded[int])  # revealed: Bounded[int]
reveal_type(Bounded[IntSubclass])  # revealed: Bounded[IntSubclass]

# TODO: update this diagnostic to talk about type parameters and specializations
# error: [invalid-argument-type] "Argument is incorrect: Expected `int`, found `str`"
reveal_type(Bounded[str])  # revealed: Unknown

# TODO: update this diagnostic to talk about type parameters and specializations
# error: [invalid-argument-type] "Argument is incorrect: Expected `int`, found `int | str`"
reveal_type(Bounded[int | str])  # revealed: Unknown

reveal_type(BoundedByUnion[int])  # revealed: BoundedByUnion[int]
reveal_type(BoundedByUnion[IntSubclass])  # revealed: BoundedByUnion[IntSubclass]
reveal_type(BoundedByUnion[str])  # revealed: BoundedByUnion[str]
reveal_type(BoundedByUnion[int | str])  # revealed: BoundedByUnion[int | str]
```

If the type variable is constrained, the specialized type must satisfy those constraints:

```py
type Constrained[T: (int, str)] = ...

reveal_type(Constrained[int])  # revealed: Constrained[int]

# TODO: error: [invalid-argument-type]
# TODO: revealed: Constrained[Unknown]
reveal_type(Constrained[IntSubclass])  # revealed: Constrained[IntSubclass]

reveal_type(Constrained[str])  # revealed: Constrained[str]

# TODO: error: [invalid-argument-type]
# TODO: revealed: Unknown
reveal_type(Constrained[int | str])  # revealed: Constrained[int | str]

# TODO: update this diagnostic to talk about type parameters and specializations
# error: [invalid-argument-type] "Argument is incorrect: Expected `int | str`, found `object`"
reveal_type(Constrained[object])  # revealed: Unknown
```

If the type variable has a default, it can be omitted:

```py
type WithDefault[T, U = int] = ...

reveal_type(WithDefault[str, str])  # revealed: WithDefault[str, str]
reveal_type(WithDefault[str])  # revealed: WithDefault[str, int]
```

If the type alias is not specialized explicitly, it is implicitly specialized to `Unknown`:

```py
type G[T] = list[T]

def _(g: G):
    reveal_type(g)  # revealed: list[Unknown]
```

Unless a type default was provided:

```py
type G[T = int] = list[T]

def _(g: G):
    reveal_type(g)  # revealed: list[int]
```

## Aliases are not callable

```py
type A = int
type B[T] = T

# error: [call-non-callable] "Object of type `TypeAliasType` is not callable"
reveal_type(A())  # revealed: Unknown

# error: [call-non-callable] "Object of type `GenericAlias` is not callable"
reveal_type(B[int]())  # revealed: Unknown
```

## Recursive Truthiness

Make sure we handle cycles correctly when computing the truthiness of a generic type alias:

```py
type X[T: X] = T

def _(x: X):
    assert x
```

## Recursive generic type aliases

```py
type RecursiveList[T] = T | list[RecursiveList[T]]

r1: RecursiveList[int] = 1
r2: RecursiveList[int] = [1, [1, 2, 3]]
# error: [invalid-assignment] "Object of type `Literal["a"]` is not assignable to `RecursiveList[int]`"
r3: RecursiveList[int] = "a"
# error: [invalid-assignment]
r4: RecursiveList[int] = ["a"]
# TODO: this should be an error
r5: RecursiveList[int] = [1, ["a"]]

def _(x: RecursiveList[int]):
    if isinstance(x, list):
        # TODO: should be `list[RecursiveList[int]]
        reveal_type(x[0])  # revealed: int | list[Any]
    if isinstance(x, list) and isinstance(x[0], list):
        # TODO: should be `list[RecursiveList[int]]`
        reveal_type(x[0])  # revealed: list[Any]
```

Assignment checks respect structural subtyping, i.e. type aliases with the same structure are
assignable to each other.

```py
# This is structurally equivalent to RecursiveList[T].
type RecursiveList2[T] = T | list[T | list[RecursiveList[T]]]
# This is not structurally equivalent to RecursiveList[T].
type RecursiveList3[T] = T | list[list[RecursiveList[T]]]

def _(x: RecursiveList[int], y: RecursiveList2[int]):
    r1: RecursiveList2[int] = x
    # error: [invalid-assignment]
    r2: RecursiveList3[int] = x

    r3: RecursiveList[int] = y
    # error: [invalid-assignment]
    r4: RecursiveList3[int] = y
```

It is also possible to handle divergent type aliases that are not actually have instances.

```py
# The type variable `T` has no meaning here, it's just to make sure it works correctly.
type DivergentList[T] = list[DivergentList[T]]

d1: DivergentList[int] = []
# error: [invalid-assignment]
d2: DivergentList[int] = [1]
# error: [invalid-assignment]
d3: DivergentList[int] = ["a"]
# TODO: this should be an error
d4: DivergentList[int] = [[1]]

def _(x: DivergentList[int]):
    d1: DivergentList[int] = [x]
    d2: DivergentList[int] = x[0]
```
