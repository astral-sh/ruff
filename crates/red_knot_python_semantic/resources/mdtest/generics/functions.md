# Generic functions

## Typevar must be used at least twice

If you're only using a typevar for a single parameter, you don't need the typevar — just use
`object` (or the typevar's upper bound):

```py
# TODO: error, should be (x: object)
def typevar_not_needed[T](x: T) -> None:
    pass

# TODO: error, should be (x: int)
def bounded_typevar_not_needed[T: int](x: T) -> None:
    pass
```

Typevars are only needed if you use them more than once. For instance, to specify that two
parameters must both have the same type:

```py
def two_params[T](x: T, y: T) -> T:
    return x
```

or to specify that a return value is the same as a parameter:

```py
def return_value[T](x: T) -> T:
    return x
```

Each typevar must also appear _somewhere_ in the parameter list:

```py
def absurd[T]() -> T:
    # There's no way to construct a T!
    raise ValueError("absurd")
```

## Inferring generic function parameter types

If the type of a generic function parameter is a typevar, then we can infer what type that typevar
is bound to at each call site.

TODO: Note that some of the TODO revealed types have two options, since we haven't decided yet
whether we want to infer a more specific `Literal` type where possible, or use heuristics to weaken
the inferred type to e.g. `int`.

```py
def f[T](x: T) -> T:
    return x

# TODO: no error
# TODO: revealed: int or Literal[1]
# error: [invalid-argument-type]
reveal_type(f(1))  # revealed: T

# TODO: no error
# TODO: revealed: float
# error: [invalid-argument-type]
reveal_type(f(1.0))  # revealed: T

# TODO: no error
# TODO: revealed: bool or Literal[true]
# error: [invalid-argument-type]
reveal_type(f(True))  # revealed: T

# TODO: no error
# TODO: revealed: str or Literal["string"]
# error: [invalid-argument-type]
reveal_type(f("string"))  # revealed: T
```

## Inferring “deep” generic parameter types

The matching up of call arguments and discovery of constraints on typevars can be a recursive
process for arbitrarily-nested generic types in parameters.

```py
def f[T](x: list[T]) -> T:
    return x[0]

# TODO: revealed: float
reveal_type(f([1.0, 2.0]))  # revealed: T
```

## Typevar constraints

If a type parameter has an upper bound, that upper bound constrains which types can be used for that
typevar. This effectively adds the upper bound as an intersection to every appearance of the typevar
in the function.

```py
def good_param[T: int](x: T) -> None:
    # TODO: revealed: T & int
    reveal_type(x)  # revealed: T
```

If the function is annotated as returning the typevar, this means that the upper bound is _not_
assignable to that typevar, since return types are contravariant. In `bad`, we can infer that
`x + 1` has type `int`. But `T` might be instantiated with a narrower type than `int`, and so the
return value is not guaranteed to be compatible for all `T: int`.

```py
def good_return[T: int](x: T) -> T:
    return x

def bad_return[T: int](x: T) -> T:
    # error: [invalid-return-type] "Object of type `int` is not assignable to return type `T`"
    return x + 1
```

## All occurrences of the same typevar have the same type

If a typevar appears multiple times in a function signature, all occurrences have the same type.

```py
def different_types[T, S](cond: bool, t: T, s: S) -> T:
    if cond:
        return t
    else:
        # error: [invalid-return-type] "Object of type `S` is not assignable to return type `T`"
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

## Typevar inference is a unification problem

When inferring typevar assignments in a generic function call, we cannot simply solve constraints
eagerly for each parameter in turn. We must solve a unification problem involving all of the
parameters simultaneously.

```py
def two_params[T](x: T, y: T) -> T:
    return x

# TODO: no error
# TODO: revealed: str
# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(two_params("a", "b"))  # revealed: T

# TODO: no error
# TODO: revealed: str | int
# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(two_params("a", 1))  # revealed: T
```

```py
def param_with_union[T](x: T | int, y: T) -> T:
    return y

# TODO: no error
# TODO: revealed: str
# error: [invalid-argument-type]
reveal_type(param_with_union(1, "a"))  # revealed: T

# TODO: no error
# TODO: revealed: str
# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(param_with_union("a", "a"))  # revealed: T

# TODO: no error
# TODO: revealed: int
# error: [invalid-argument-type]
reveal_type(param_with_union(1, 1))  # revealed: T

# TODO: no error
# TODO: revealed: str | int
# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(param_with_union("a", 1))  # revealed: T
```

```py
def tuple_param[T, S](x: T | S, y: tuple[T, S]) -> tuple[T, S]:
    return y

# TODO: no error
# TODO: revealed: tuple[str, int]
# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(tuple_param("a", ("a", 1)))  # revealed: tuple[T, S]

# TODO: no error
# TODO: revealed: tuple[str, int]
# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(tuple_param(1, ("a", 1)))  # revealed: tuple[T, S]
```

## Inferring nested generic function calls

We can infer type assignments in nested calls to multiple generic functions. If they use the same
type variable, we do not confuse the two; `T@f` and `T@g` have separate types in each example below.

```py
def f[T](x: T) -> tuple[T, int]:
    return (x, 1)

def g[T](x: T) -> T | None:
    return x

# TODO: no error
# TODO: revealed: tuple[str | None, int]
# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(f(g("a")))  # revealed: tuple[T, int]

# TODO: no error
# TODO: revealed: tuple[str, int] | None
# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(g(f("a")))  # revealed: T | None
```
