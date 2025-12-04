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

# revealed: ty_extensions.GenericContext[T@SingleTypevar]
reveal_type(generic_context(SingleTypevar))
# revealed: ty_extensions.GenericContext[T@MultipleTypevars, S@MultipleTypevars]
reveal_type(generic_context(MultipleTypevars))

# TODO: support `TypeVarTuple` properly
# (these should include the `TypeVarTuple`s in their generic contexts)
# revealed: ty_extensions.GenericContext[P@SingleParamSpec]
reveal_type(generic_context(SingleParamSpec))
# revealed: ty_extensions.GenericContext[T@TypeVarAndParamSpec, P@TypeVarAndParamSpec]
reveal_type(generic_context(TypeVarAndParamSpec))
# revealed: ty_extensions.GenericContext[]
reveal_type(generic_context(SingleTypeVarTuple))
# revealed: ty_extensions.GenericContext[T@TypeVarAndTypeVarTuple]
reveal_type(generic_context(TypeVarAndTypeVarTuple))
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
# error: [invalid-type-arguments] "Too many type arguments: expected 1, got 2"
reveal_type(C[int, int])  # revealed: <type alias 'C[Unknown]'>
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

reveal_type(Bounded[int])  # revealed: <type alias 'Bounded[int]'>
reveal_type(Bounded[IntSubclass])  # revealed: <type alias 'Bounded[IntSubclass]'>

# error: [invalid-type-arguments] "Type `str` is not assignable to upper bound `int` of type variable `T@Bounded`"
reveal_type(Bounded[str])  # revealed: <type alias 'Bounded[Unknown]'>

# error: [invalid-type-arguments] "Type `int | str` is not assignable to upper bound `int` of type variable `T@Bounded`"
reveal_type(Bounded[int | str])  # revealed: <type alias 'Bounded[Unknown]'>

reveal_type(BoundedByUnion[int])  # revealed: <type alias 'BoundedByUnion[int]'>
reveal_type(BoundedByUnion[IntSubclass])  # revealed: <type alias 'BoundedByUnion[IntSubclass]'>
reveal_type(BoundedByUnion[str])  # revealed: <type alias 'BoundedByUnion[str]'>
reveal_type(BoundedByUnion[int | str])  # revealed: <type alias 'BoundedByUnion[int | str]'>
```

If the type variable is constrained, the specialized type must satisfy those constraints:

```py
type Constrained[T: (int, str)] = ...

reveal_type(Constrained[int])  # revealed: <type alias 'Constrained[int]'>

# TODO: error: [invalid-argument-type]
# TODO: revealed: Constrained[Unknown]
reveal_type(Constrained[IntSubclass])  # revealed: <type alias 'Constrained[IntSubclass]'>

reveal_type(Constrained[str])  # revealed: <type alias 'Constrained[str]'>

# TODO: error: [invalid-argument-type]
# TODO: revealed: Unknown
reveal_type(Constrained[int | str])  # revealed: <type alias 'Constrained[int | str]'>

# error: [invalid-type-arguments] "Type `object` does not satisfy constraints `int`, `str` of type variable `T@Constrained`"
reveal_type(Constrained[object])  # revealed: <type alias 'Constrained[Unknown]'>
```

If the type variable has a default, it can be omitted:

```py
type WithDefault[T, U = int] = ...

reveal_type(WithDefault[str, str])  # revealed: <type alias 'WithDefault[str, str]'>
reveal_type(WithDefault[str])  # revealed: <type alias 'WithDefault[str, int]'>
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
