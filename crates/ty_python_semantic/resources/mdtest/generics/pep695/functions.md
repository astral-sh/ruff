# Generic functions: PEP 695 syntax

```toml
[environment]
python-version = "3.12"
```

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

```py
def f[T](x: T) -> T:
    return x

reveal_type(f(1))  # revealed: Literal[1]
reveal_type(f(1.0))  # revealed: float
reveal_type(f(True))  # revealed: Literal[True]
reveal_type(f("string"))  # revealed: Literal["string"]
```

## Inferring “deep” generic parameter types

The matching up of call arguments and discovery of constraints on typevars can be a recursive
process for arbitrarily-nested generic classes and protocols in parameters.

TODO: Note that we can currently only infer a specialization for a generic protocol when the
argument _explicitly_ implements the protocol by listing it as a base class.

```py
from typing import Protocol, TypeVar

S = TypeVar("S")

class CanIndex(Protocol[S]):
    def __getitem__(self, index: int, /) -> S: ...

class ExplicitlyImplements[T](CanIndex[T]): ...

def takes_in_list[T](x: list[T]) -> list[T]:
    return x

def takes_in_protocol[T](x: CanIndex[T]) -> T:
    return x[0]

def deep_list(x: list[str]) -> None:
    reveal_type(takes_in_list(x))  # revealed: list[str]
    # TODO: revealed: str
    reveal_type(takes_in_protocol(x))  # revealed: Unknown

def deeper_list(x: list[set[str]]) -> None:
    reveal_type(takes_in_list(x))  # revealed: list[set[str]]
    # TODO: revealed: set[str]
    reveal_type(takes_in_protocol(x))  # revealed: Unknown

def deep_explicit(x: ExplicitlyImplements[str]) -> None:
    reveal_type(takes_in_protocol(x))  # revealed: str

def deeper_explicit(x: ExplicitlyImplements[set[str]]) -> None:
    reveal_type(takes_in_protocol(x))  # revealed: set[str]

def takes_in_type[T](x: type[T]) -> type[T]:
    return x

reveal_type(takes_in_type(int))  # revealed: type[int]
```

This also works when passing in arguments that are subclasses of the parameter type.

```py
class Sub(list[int]): ...
class GenericSub[T](list[T]): ...

reveal_type(takes_in_list(Sub()))  # revealed: list[int]
# TODO: revealed: int
reveal_type(takes_in_protocol(Sub()))  # revealed: Unknown

reveal_type(takes_in_list(GenericSub[str]()))  # revealed: list[str]
# TODO: revealed: str
reveal_type(takes_in_protocol(GenericSub[str]()))  # revealed: Unknown

class ExplicitSub(ExplicitlyImplements[int]): ...
class ExplicitGenericSub[T](ExplicitlyImplements[T]): ...

reveal_type(takes_in_protocol(ExplicitSub()))  # revealed: int
reveal_type(takes_in_protocol(ExplicitGenericSub[str]()))  # revealed: str
```

## Inferring tuple parameter types

```py
def takes_mixed_tuple_suffix[T](x: tuple[int, bytes, *tuple[str, ...], T, int]) -> T:
    return x[-2]

def takes_mixed_tuple_prefix[T](x: tuple[int, T, *tuple[str, ...], bool, int]) -> T:
    return x[1]

def _(x: tuple[int, bytes, *tuple[str, ...], bool, int]):
    reveal_type(takes_mixed_tuple_suffix(x))  # revealed: bool
    reveal_type(takes_mixed_tuple_prefix(x))  # revealed: bytes

reveal_type(takes_mixed_tuple_suffix((1, b"foo", "bar", "baz", True, 42)))  # revealed: Literal[True]
reveal_type(takes_mixed_tuple_prefix((1, b"foo", "bar", "baz", True, 42)))  # revealed: Literal[b"foo"]

def takes_fixed_tuple[T](x: tuple[T, int]) -> T:
    return x[0]

def _(x: tuple[str, int]):
    reveal_type(takes_fixed_tuple(x))  # revealed: str

reveal_type(takes_fixed_tuple((True, 42)))  # revealed: Literal[True]

def takes_homogeneous_tuple[T](x: tuple[T, ...]) -> T:
    return x[0]

def _(x: tuple[str, int], y: tuple[bool, ...], z: tuple[int, str, *tuple[range, ...], bytes]):
    reveal_type(takes_homogeneous_tuple(x))  # revealed: str | int
    reveal_type(takes_homogeneous_tuple(y))  # revealed: bool
    reveal_type(takes_homogeneous_tuple(z))  # revealed: int | str | range | bytes

reveal_type(takes_homogeneous_tuple((42,)))  # revealed: Literal[42]
reveal_type(takes_homogeneous_tuple((42, 43)))  # revealed: Literal[42, 43]
```

## Inferring a bound typevar

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

def f[T: int](x: T) -> T:
    return x

reveal_type(f(1))  # revealed: Literal[1]
reveal_type(f(True))  # revealed: Literal[True]
# error: [invalid-argument-type]
reveal_type(f("string"))  # revealed: Unknown
```

## Inferring a constrained typevar

<!-- snapshot-diagnostics -->

```py
from typing_extensions import reveal_type

def f[T: (int, None)](x: T) -> T:
    return x

reveal_type(f(1))  # revealed: int
reveal_type(f(True))  # revealed: int
reveal_type(f(None))  # revealed: None
# error: [invalid-argument-type]
reveal_type(f("string"))  # revealed: Unknown
```

## Typevar constraints

If a type parameter has an upper bound, that upper bound constrains which types can be used for that
typevar. This effectively adds the upper bound as an intersection to every appearance of the typevar
in the function.

```py
def good_param[T: int](x: T) -> None:
    reveal_type(x)  # revealed: T@good_param
```

If the function is annotated as returning the typevar, this means that the upper bound is _not_
assignable to that typevar, since return types are contravariant. In `bad`, we can infer that
`x + 1` has type `int`. But `T` might be instantiated with a narrower type than `int`, and so the
return value is not guaranteed to be compatible for all `T: int`.

```py
def good_return[T: int](x: T) -> T:
    return x

def bad_return[T: int](x: T) -> T:
    # error: [invalid-return-type] "Return type does not match returned value: expected `T@bad_return`, found `int`"
    return x + 1
```

## All occurrences of the same typevar have the same type

If a typevar appears multiple times in a function signature, all occurrences have the same type.

```py
def different_types[T, S](cond: bool, t: T, s: S) -> T:
    if cond:
        return t
    else:
        # error: [invalid-return-type] "Return type does not match returned value: expected `T@different_types`, found `S@different_types`"
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
    # error: [unsupported-operator] "Operator `+` is unsupported between objects of type `T@same_constrained_types` and `T@same_constrained_types`"
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

reveal_type(two_params("a", "b"))  # revealed: Literal["a", "b"]
reveal_type(two_params("a", 1))  # revealed: Literal["a", 1]
```

When one of the parameters is a union, we attempt to find the smallest specialization that satisfies
all of the constraints.

```py
def union_param[T](x: T | None) -> T:
    if x is None:
        raise ValueError
    return x

reveal_type(union_param("a"))  # revealed: Literal["a"]
reveal_type(union_param(1))  # revealed: Literal[1]
reveal_type(union_param(None))  # revealed: Unknown

def _(x: int | None):
    reveal_type(union_param(x))  # revealed: int
```

```py
def union_and_nonunion_params[T](x: T | int, y: T) -> T:
    return y

reveal_type(union_and_nonunion_params(1, "a"))  # revealed: Literal["a"]
reveal_type(union_and_nonunion_params("a", "a"))  # revealed: Literal["a"]
reveal_type(union_and_nonunion_params(1, 1))  # revealed: Literal[1]
reveal_type(union_and_nonunion_params(3, 1))  # revealed: Literal[1]
reveal_type(union_and_nonunion_params("a", 1))  # revealed: Literal["a", 1]
```

```py
def tuple_param[T, S](x: T | S, y: tuple[T, S]) -> tuple[T, S]:
    return y

reveal_type(tuple_param("a", ("a", 1)))  # revealed: tuple[Literal["a"], Literal[1]]
reveal_type(tuple_param(1, ("a", 1)))  # revealed: tuple[Literal["a"], Literal[1]]
```

## Inferring nested generic function calls

We can infer type assignments in nested calls to multiple generic functions. If they use the same
type variable, we do not confuse the two; `T@f` and `T@g` have separate types in each example below.

```py
def f[T](x: T) -> tuple[T, int]:
    return (x, 1)

def g[T](x: T) -> T | None:
    return x

reveal_type(f(g("a")))  # revealed: tuple[Literal["a"] | None, int]
reveal_type(g(f("a")))  # revealed: tuple[Literal["a"], int] | None
```

## Passing generic functions to generic functions

```py
from typing import Callable

def invoke[A, B](fn: Callable[[A], B], value: A) -> B:
    return fn(value)

def identity[T](x: T) -> T:
    return x

def head[T](xs: list[T]) -> T:
    return xs[0]

# TODO: this should be `Literal[1]`
reveal_type(invoke(identity, 1))  # revealed: Unknown

# TODO: this should be `Unknown | int`
reveal_type(invoke(head, [1, 2, 3]))  # revealed: Unknown
```

## Protocols as TypeVar bounds

Protocol types can be used as TypeVar bounds, just like nominal types.

```py
from typing import Any, Protocol
from ty_extensions import static_assert, is_assignable_to

class SupportsClose(Protocol):
    def close(self) -> None: ...

class ClosableFullyStaticProtocol(Protocol):
    x: int
    def close(self) -> None: ...

class ClosableNonFullyStaticProtocol(Protocol):
    x: Any
    def close(self) -> None: ...

class ClosableFullyStaticNominal:
    x: int
    def close(self) -> None: ...

class ClosableNonFullyStaticNominal:
    x: int
    def close(self) -> None: ...

class NotClosableProtocol(Protocol): ...
class NotClosableNominal: ...

def close_and_return[T: SupportsClose](x: T) -> T:
    x.close()
    return x

def f(
    a: SupportsClose,
    b: ClosableFullyStaticProtocol,
    c: ClosableNonFullyStaticProtocol,
    d: ClosableFullyStaticNominal,
    e: ClosableNonFullyStaticNominal,
    f: NotClosableProtocol,
    g: NotClosableNominal,
):
    reveal_type(close_and_return(a))  # revealed: SupportsClose
    reveal_type(close_and_return(b))  # revealed: ClosableFullyStaticProtocol
    reveal_type(close_and_return(c))  # revealed: ClosableNonFullyStaticProtocol
    reveal_type(close_and_return(d))  # revealed: ClosableFullyStaticNominal
    reveal_type(close_and_return(e))  # revealed: ClosableNonFullyStaticNominal

    # error: [invalid-argument-type] "does not satisfy upper bound"
    reveal_type(close_and_return(f))  # revealed: Unknown
    # error: [invalid-argument-type] "does not satisfy upper bound"
    reveal_type(close_and_return(g))  # revealed: Unknown
```

## Opaque decorators don't affect typevar binding

Inside the body of a generic function, we should be able to see that the typevars bound by that
function are in fact bound by that function. This requires being able to see the enclosing
function's _undecorated_ type and signature, especially in the case where a gradually typed
decorator "hides" the function type from outside callers.

```py
from typing import cast, Any, Callable

def opaque_decorator(f: Any) -> Any:
    return f

def transparent_decorator[F: Callable[..., Any]](f: F) -> F:
    return f

@opaque_decorator
def decorated[T](t: T) -> None:
    # error: [redundant-cast]
    reveal_type(cast(T, t))  # revealed: T@decorated

@transparent_decorator
def decorated[T](t: T) -> None:
    # error: [redundant-cast]
    reveal_type(cast(T, t))  # revealed: T@decorated
```

## Solving TypeVars with upper bounds in unions

```py
class A: ...

class B[T: A]:
    x: T

def f[T: A](c: T | None):
    return None

def g[T: A](b: B[T]):
    return f(b.x)  # Fine
```

## Typevars in a union

```py
def takes_in_union[T](t: T | None) -> T:
    raise NotImplementedError

def takes_in_bigger_union[T](t: T | int | None) -> T:
    raise NotImplementedError

def _(x: str | None) -> None:
    reveal_type(takes_in_union(x))  # revealed: str
    reveal_type(takes_in_bigger_union(x))  # revealed: str

def _(x: str | int | None) -> None:
    reveal_type(takes_in_union(x))  # revealed: str | int
    reveal_type(takes_in_bigger_union(x))  # revealed: str
```

This is a regression test for an issue that surfaced in the primer report of an early version of
<https://github.com/astral-sh/ruff/pull/19811>, where we failed to solve the `TypeVar` here due to
the fact that it only appears in the function's type annotations as part of a union:

```py
def f[T: (str, bytes)](suffix: T | None, prefix: T | None):
    return None

def g(x: str):
    f(prefix=x, suffix=".tar.gz")
```

If the type variable is present multiple times in the union, we choose the correct union element to
infer against based on the argument type:

```py
def h[T](x: list[T] | dict[T, T]) -> T | None: ...
def _(x: list[int], y: dict[int, int]):
    reveal_type(h(x))  # revealed: int | None
    reveal_type(h(y))  # revealed: int | None
```

## Nested functions see typevars bound in outer function

```py
from typing import overload

def outer[T](t: T) -> None:
    def inner[T](t: T) -> None: ...

    inner(t)

@overload
def overloaded_outer() -> None: ...
@overload
def overloaded_outer[T](t: T) -> None: ...
def overloaded_outer[T](t: T | None = None) -> None:
    def inner(t: T) -> None: ...

    if t is not None:
        inner(t)

def outer[T](t: T) -> None:
    def inner[S](inner_t: T, s: S) -> tuple[T, S]:
        return inner_t, s
    reveal_type(inner(t, 1))  # revealed: tuple[T@outer, Literal[1]]

    inner("wrong", 1)  # error: [invalid-argument-type]
```

## Unpacking a TypeVar

We can infer precise heterogeneous types from the result of an unpacking operation applied to a
TypeVar if the TypeVar's upper bound is a type with a precise tuple spec:

```py
from dataclasses import dataclass
from typing import NamedTuple, Final

def f[T: tuple[int, str]](x: T) -> T:
    a, b = x
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: str
    return x

@dataclass
class Team[T: tuple[int, str]]:
    employees: list[T]

def x[T: tuple[int, str]](team: Team[T]) -> Team[T]:
    age, name = team.employees[0]
    reveal_type(age)  # revealed: int
    reveal_type(name)  # revealed: str
    return team

class Age(int): ...
class Name(str): ...

class Employee(NamedTuple):
    age: Age
    name: Name

EMPLOYEES: Final = (Employee(name=Name("alice"), age=Age(42)),)
team = Team(employees=list(EMPLOYEES))
reveal_type(team.employees)  # revealed: list[Employee]
age, name = team.employees[0]
reveal_type(age)  # revealed: Age
reveal_type(name)  # revealed: Name
```

## `self` in PEP 695 generic methods

When a generic method uses a PEP 695 generic context, an implict or explicit annotation of
`self: Self` is still part of the full generic context:

```py
from typing import Self

class C:
    def explicit_self[T](self: Self, x: T) -> tuple[Self, T]:
        return self, x

    def implicit_self[T](self, x: T) -> tuple[Self, T]:
        return self, x

def _(x: int):
    reveal_type(C().explicit_self(x))  # revealed: tuple[C, int]

    reveal_type(C().implicit_self(x))  # revealed: tuple[C, int]
```

## `~T` is never assignable to `T`

```py
from ty_extensions import Not

def f[T](x: T, y: Not[T]) -> T:
    x = y  # error: [invalid-assignment]
    y = x  # error: [invalid-assignment]
    return x
```
