# Generic functions

## Inferring generic function parameter types

If the type of a generic function parameter is a typevar, then we can infer what type that typevar
is bound to at each call site.

```py
def f[T](x: T) -> T: ...

# TODO: no error
# TODO: revealed: int
# error: [invalid-argument-type]
reveal_type(f(1))  # revealed: T

# TODO: no error
# TODO: revealed: float
# error: [invalid-argument-type]
reveal_type(f(1.0))  # revealed: T

# TODO: no error
# TODO: revealed: bool
# error: [invalid-argument-type]
reveal_type(f(True))  # revealed: T

# TODO: no error
# TODO: revealed: str
# error: [invalid-argument-type]
reveal_type(f("string"))  # revealed: T
```

## Inferring “deep” generic parameter types

The matching up of call arguments and discovery of constraints on typevars can be a recursive
process for arbitrarily-nested generic types in parameters.

```py
def f[T](x: list[T]) -> T: ...

# TODO: revealed: float
reveal_type(f([1.0, 2.0]))  # revealed: T
```

## Return type uses actual typevar, not upper bound

If a function is annotated as returning a typevar, the return type must be an instance of the actual
typevar, and not just something assignable to its upper bound.

In `bad`, we can infer that `x + 1` has type `int`, since `int.__add__` is defined to return `int`,
and Liskov requires that any subtype `T` of `int` has a compatible implementation of `__add__`. But
`T` might be instantiated with a narrower type than `int`, and so the return value is not guaranteed
to be compatible for all `T: int`.

```py
def good[T: int](x: T) -> T:
    return x

def bad[T: int](x: T) -> T:
    # TODO: error: int is not assignable to T
    # error: [unsupported-operator] "Operator `+` is unsupported between objects of type `T` and `Literal[1]`"
    return x + 1
```

## All occurrences of the same typevar have the same type

If a typevar appears multiple times in a function signature, all occurrences have the same type.

```py
def different_types[T, S](cond: bool, t: T, s: S) -> T:
    if cond:
        return t
    else:
        # TODO: error: S is not assignable to T
        return s

def same_types[T](cond: bool, t1: T, t2: T) -> T:
    if cond:
        return t1
    else:
        return t2
```

## All occurrences of the same constrained typevar have the same type

The above is true even when the typevars are constrained. Here, both `int` and `str` have `__add__`
methods that are compatible with the return type, so the `return` expression is always well-typed:

```py
def same_constrained_types[T: (int, str)](t1: T, t2: T) -> T:
    # TODO: no error
    # error: [unsupported-operator] "Operator `+` is unsupported between objects of type `T` and `T`"
    return t1 + t2
```

This is _not_ the same as a union type, because of this additional constraint that the two
occurrences have the same type. In `unions_are_different`, `t1` and `t2` might have different types,
and an `int` and a `str` cannot be added together:

```py
def unions_are_different(t1: int | str, t2: int | str) -> int | str:
    # error: [unsupported-operator] "Operator `+` is unsupported between objects of type `int | str` and `int | str`"
    return t1 + t2
```
