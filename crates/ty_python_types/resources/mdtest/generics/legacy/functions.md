# Generic functions: Legacy syntax

## Typevar must be used at least twice

If you're only using a typevar for a single parameter, you don't need the typevar — just use
`object` (or the typevar's upper bound):

```py
from typing import TypeVar

T = TypeVar("T")

# TODO: error, should be (x: object)
def typevar_not_needed(x: T) -> None:
    pass

BoundedT = TypeVar("BoundedT", bound=int)

# TODO: error, should be (x: int)
def bounded_typevar_not_needed(x: BoundedT) -> None:
    pass
```

Typevars are only needed if you use them more than once. For instance, to specify that two
parameters must both have the same type:

```py
def two_params(x: T, y: T) -> T:
    return x
```

or to specify that a return value is the same as a parameter:

```py
def return_value(x: T) -> T:
    return x
```

Each typevar must also appear _somewhere_ in the parameter list:

```py
def absurd() -> T:
    # There's no way to construct a T!
    raise ValueError("absurd")
```

## Inferring generic function parameter types

If the type of a generic function parameter is a typevar, then we can infer what type that typevar
is bound to at each call site.

```py
from typing import TypeVar

T = TypeVar("T")

def f(x: T) -> T:
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

T = TypeVar("T")

class CanIndex(Protocol[T]):
    def __getitem__(self, index: int, /) -> T: ...

class ExplicitlyImplements(CanIndex[T]): ...
class SubProtocol(CanIndex[T], Protocol): ...

def takes_in_list(x: list[T]) -> list[T]:
    return x

def takes_in_protocol(x: CanIndex[T]) -> T:
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

def deep_subprotocol(x: SubProtocol[str]) -> None:
    reveal_type(takes_in_protocol(x))  # revealed: str

def deeper_subprotocol(x: SubProtocol[set[str]]) -> None:
    reveal_type(takes_in_protocol(x))  # revealed: set[str]

def itself(x: CanIndex[str]) -> None:
    reveal_type(takes_in_protocol(x))  # revealed: str

def deep_itself(x: CanIndex[set[str]]) -> None:
    reveal_type(takes_in_protocol(x))  # revealed: set[str]

def takes_in_type(x: type[T]) -> type[T]:
    return x

reveal_type(takes_in_type(int))  # revealed: type[int]
```

This also works when passing in arguments that are subclasses of the parameter type.

```py
class Sub(list[int]): ...
class GenericSub(list[T]): ...

reveal_type(takes_in_list(Sub()))  # revealed: list[int]
# TODO: revealed: int
reveal_type(takes_in_protocol(Sub()))  # revealed: Unknown

reveal_type(takes_in_list(GenericSub[str]()))  # revealed: list[str]
# TODO: revealed: str
reveal_type(takes_in_protocol(GenericSub[str]()))  # revealed: Unknown

class ExplicitSub(ExplicitlyImplements[int]): ...
class ExplicitGenericSub(ExplicitlyImplements[T]): ...

reveal_type(takes_in_protocol(ExplicitSub()))  # revealed: int
reveal_type(takes_in_protocol(ExplicitGenericSub[str]()))  # revealed: str
```

## Inferring tuple parameter types

```toml
[environment]
python-version = "3.12"
```

```py
from typing import TypeVar

T = TypeVar("T")

def takes_mixed_tuple_suffix(x: tuple[int, bytes, *tuple[str, ...], T, int]) -> T:
    return x[-2]

def takes_mixed_tuple_prefix(x: tuple[int, T, *tuple[str, ...], bool, int]) -> T:
    return x[1]

def _(x: tuple[int, bytes, *tuple[str, ...], bool, int]):
    reveal_type(takes_mixed_tuple_suffix(x))  # revealed: bool
    reveal_type(takes_mixed_tuple_prefix(x))  # revealed: bytes

reveal_type(takes_mixed_tuple_suffix((1, b"foo", "bar", "baz", True, 42)))  # revealed: Literal[True]
reveal_type(takes_mixed_tuple_prefix((1, b"foo", "bar", "baz", True, 42)))  # revealed: Literal[b"foo"]

def takes_fixed_tuple(x: tuple[T, int]) -> T:
    return x[0]

def _(x: tuple[str, int]):
    reveal_type(takes_fixed_tuple(x))  # revealed: str

reveal_type(takes_fixed_tuple((True, 42)))  # revealed: Literal[True]

def takes_homogeneous_tuple(x: tuple[T, ...]) -> T:
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
from typing import TypeVar

T = TypeVar("T", bound=int)

def f(x: T) -> T:
    return x

reveal_type(f(1))  # revealed: Literal[1]
reveal_type(f(True))  # revealed: Literal[True]
# error: [invalid-argument-type]
reveal_type(f("string"))  # revealed: Unknown
```

## Inferring a constrained typevar

<!-- snapshot-diagnostics -->

```py
from typing import TypeVar

T = TypeVar("T", int, None)

def f(x: T) -> T:
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
from typing import TypeVar

T = TypeVar("T", bound=int)

def good_param(x: T) -> None:
    reveal_type(x)  # revealed: T@good_param
```

If the function is annotated as returning the typevar, this means that the upper bound is _not_
assignable to that typevar, since return types are contravariant. In `bad`, we can infer that
`x + 1` has type `int`. But `T` might be instantiated with a narrower type than `int`, and so the
return value is not guaranteed to be compatible for all `T: int`.

```py
def good_return(x: T) -> T:
    return x

def bad_return(x: T) -> T:
    # error: [invalid-return-type] "Return type does not match returned value: expected `T@bad_return`, found `int`"
    return x + 1
```

## All occurrences of the same typevar have the same type

If a typevar appears multiple times in a function signature, all occurrences have the same type.

```py
from typing import TypeVar

T = TypeVar("T")
S = TypeVar("S")

def different_types(cond: bool, t: T, s: S) -> T:
    if cond:
        return t
    else:
        # error: [invalid-return-type] "Return type does not match returned value: expected `T@different_types`, found `S@different_types`"
        return s

def same_types(cond: bool, t1: T, t2: T) -> T:
    if cond:
        return t1
    else:
        return t2
```

## All occurrences of the same constrained typevar have the same type

The above is true even when the typevars are constrained. Here, both `int` and `str` have `__add__`
methods that are compatible with the return type, so the `return` expression is always well-typed:

```py
from typing import TypeVar

T = TypeVar("T", int, str)

def same_constrained_types(t1: T, t2: T) -> T:
    # TODO: no error
    # error: [unsupported-operator] "Operator `+` is not supported between two objects of type `T@same_constrained_types`"
    return t1 + t2
```

This is _not_ the same as a union type, because of this additional constraint that the two
occurrences have the same type. In `unions_are_different`, `t1` and `t2` might have different types,
and an `int` and a `str` cannot be added together:

```py
def unions_are_different(t1: int | str, t2: int | str) -> int | str:
    # error: [unsupported-operator] "Operator `+` is not supported between two objects of type `int | str`"
    return t1 + t2
```

## Typevar inference is a unification problem

When inferring typevar assignments in a generic function call, we cannot simply solve constraints
eagerly for each parameter in turn. We must solve a unification problem involving all of the
parameters simultaneously.

```py
from typing import TypeVar

T = TypeVar("T")

def two_params(x: T, y: T) -> T:
    return x

reveal_type(two_params("a", "b"))  # revealed: Literal["a", "b"]
reveal_type(two_params("a", 1))  # revealed: Literal["a", 1]
```

When one of the parameters is a union, we attempt to find the smallest specialization that satisfies
all of the constraints.

```py
def union_param(x: T | None) -> T:
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
def union_and_nonunion_params(x: T | int, y: T) -> T:
    return y

reveal_type(union_and_nonunion_params(1, "a"))  # revealed: Literal["a"]
reveal_type(union_and_nonunion_params("a", "a"))  # revealed: Literal["a"]
reveal_type(union_and_nonunion_params(1, 1))  # revealed: Literal[1]
reveal_type(union_and_nonunion_params(3, 1))  # revealed: Literal[1]
reveal_type(union_and_nonunion_params("a", 1))  # revealed: Literal["a", 1]
```

This also works if the typevar has a bound:

```py
T_str = TypeVar("T_str", bound=str)

def accepts_t_or_int(x: T_str | int) -> T_str:
    raise NotImplementedError

reveal_type(accepts_t_or_int("a"))  # revealed: Literal["a"]
reveal_type(accepts_t_or_int(1))  # revealed: Unknown

class Unrelated: ...

# error: [invalid-argument-type] "Argument type `Unrelated` does not satisfy upper bound `str` of type variable `T_str`"
reveal_type(accepts_t_or_int(Unrelated()))  # revealed: Unknown
```

```py
T_str = TypeVar("T_str", bound=str)

def accepts_t_or_list_of_t(x: T_str | list[T_str]) -> T_str:
    raise NotImplementedError

reveal_type(accepts_t_or_list_of_t("a"))  # revealed: Literal["a"]
# error: [invalid-argument-type] "Argument type `Literal[1]` does not satisfy upper bound `str` of type variable `T_str`"
reveal_type(accepts_t_or_list_of_t(1))  # revealed: Unknown

def _(list_ofstr: list[str], list_of_int: list[int]):
    reveal_type(accepts_t_or_list_of_t(list_ofstr))  # revealed: str

    # TODO: the error message here could be improved by referring to the second union element
    # error: [invalid-argument-type] "Argument type `list[int]` does not satisfy upper bound `str` of type variable `T_str`"
    reveal_type(accepts_t_or_list_of_t(list_of_int))  # revealed: Unknown
```

Here, we make sure that `S` is solved as `Literal[1]` instead of a union of the two literals, which
would also be a valid solution:

```py
S = TypeVar("S")

def tuple_param(x: T | S, y: tuple[T, S]) -> tuple[T, S]:
    return y

reveal_type(tuple_param("a", ("a", 1)))  # revealed: tuple[Literal["a"], Literal[1]]
reveal_type(tuple_param(1, ("a", 1)))  # revealed: tuple[Literal["a"], Literal[1]]
```

When a union parameter contains generic classes like `P[T] | Q[T]`, we can infer the typevar from
the actual argument even for non-final classes.

```py
from typing import TypeVar, Generic

T = TypeVar("T")

class P(Generic[T]):
    x: T

class Q(Generic[T]):
    x: T

def extract_t(x: P[T] | Q[T]) -> T:
    raise NotImplementedError

reveal_type(extract_t(P[int]()))  # revealed: int
reveal_type(extract_t(Q[str]()))  # revealed: str
```

Passing anything else results in an error:

```py
# error: [invalid-argument-type]
reveal_type(extract_t([1, 2]))  # revealed: Unknown
```

This also works when different union elements have different typevars:

```py
S = TypeVar("S")

def extract_both(x: P[T] | Q[S]) -> tuple[T, S]:
    raise NotImplementedError

reveal_type(extract_both(P[int]()))  # revealed: tuple[int, Unknown]
reveal_type(extract_both(Q[str]()))  # revealed: tuple[Unknown, str]
```

Inference also works when passing subclasses of the generic classes in the union.

```py
class SubP(P[T]):
    pass

class SubQ(Q[T]):
    pass

reveal_type(extract_t(SubP[int]()))  # revealed: int
reveal_type(extract_t(SubQ[str]()))  # revealed: str

reveal_type(extract_both(SubP[int]()))  # revealed: tuple[int, Unknown]
reveal_type(extract_both(SubQ[str]()))  # revealed: tuple[Unknown, str]
```

When a type is a subclass of both `P` and `Q` with different specializations, we cannot infer a
single type for `T` in `extract_t`, because `P` and `Q` are invariant. However, we can still infer
both types in a call to `extract_both`:

```py
class PandQ(P[int], Q[str]):
    pass

# TODO: Ideally, we would return `Unknown` here.
# error: [invalid-argument-type]
reveal_type(extract_t(PandQ()))  # revealed: int | str

reveal_type(extract_both(PandQ()))  # revealed: tuple[int, str]
```

When non-generic types are part of the union, we can still infer typevars for the remaining generic
types:

```py
def extract_optional_t(x: None | P[T]) -> T:
    raise NotImplementedError

reveal_type(extract_optional_t(None))  # revealed: Unknown
reveal_type(extract_optional_t(P[int]()))  # revealed: int
```

Passing anything else results in an error:

```py
# error: [invalid-argument-type]
reveal_type(extract_optional_t(Q[str]()))  # revealed: Unknown
```

If the union contains contains parent and child of a generic class, we ideally pick the union
element that is more precise:

```py
class Base(Generic[T]):
    x: T

class Sub(Base[T]): ...

def f(t: Base[T] | Sub[T | None]) -> T:
    raise NotImplementedError

reveal_type(f(Base[int]()))  # revealed: int
# TODO: Should ideally be `str`
reveal_type(f(Sub[str | None]()))  # revealed: str | None
```

If we have a case like the following, where only one of the union elements matches due to the
typevar bound, we do not emit a specialization error:

```py
from typing import TypeVar

I_int = TypeVar("I_int", bound=int)
S_str = TypeVar("S_str", bound=str)

class P(Generic[T]):
    value: T

def f(t: P[I_int] | P[S_str]) -> tuple[I_int, S_str]:
    raise NotImplementedError

reveal_type(f(P[int]()))  # revealed: tuple[int, Unknown]
reveal_type(f(P[str]()))  # revealed: tuple[Unknown, str]
```

However, if we pass something that does not match _any_ union element, we do emit an error:

```py
# error: [invalid-argument-type]
reveal_type(f(P[bytes]()))  # revealed: tuple[Unknown, Unknown]
```

## Inferring nested generic function calls

We can infer type assignments in nested calls to multiple generic functions. If they use the same
type variable, we do not confuse the two; `T@f` and `T@g` have separate types in each example below.

```py
from typing import TypeVar

T = TypeVar("T")

def f(x: T) -> tuple[T, int]:
    return (x, 1)

def g(x: T) -> T | None:
    return x

reveal_type(f(g("a")))  # revealed: tuple[Literal["a"] | None, int]
reveal_type(g(f("a")))  # revealed: tuple[Literal["a"], int] | None
```

## Passing generic functions to generic functions

```py
from typing import Callable, TypeVar

A = TypeVar("A")
B = TypeVar("B")
T = TypeVar("T")

def invoke(fn: Callable[[A], B], value: A) -> B:
    return fn(value)

def identity(x: T) -> T:
    return x

def head(xs: list[T]) -> T:
    return xs[0]

reveal_type(invoke(identity, 1))  # revealed: Literal[1]

# TODO: this should be `Unknown | int`
reveal_type(invoke(head, [1, 2, 3]))  # revealed: Unknown
```

## Opaque decorators don't affect typevar binding

Inside the body of a generic function, we should be able to see that the typevars bound by that
function are in fact bound by that function. This requires being able to see the enclosing
function's _undecorated_ type and signature, especially in the case where a gradually typed
decorator "hides" the function type from outside callers.

```py
from typing import cast, Any, Callable, TypeVar

F = TypeVar("F", bound=Callable[..., Any])
T = TypeVar("T")

def opaque_decorator(f: Any) -> Any:
    return f

def transparent_decorator(f: F) -> F:
    return f

@opaque_decorator
def decorated(t: T) -> None:
    # error: [redundant-cast]
    reveal_type(cast(T, t))  # revealed: T@decorated

@transparent_decorator
def decorated(t: T) -> None:
    # error: [redundant-cast]
    reveal_type(cast(T, t))  # revealed: T@decorated
```

## Solving TypeVars with upper bounds in unions

```py
from typing import Generic, TypeVar

class A: ...

T = TypeVar("T", bound=A)

class B(Generic[T]):
    x: T

def f(c: T | None):
    return None

def g(b: B[T]):
    return f(b.x)  # Fine
```

## Constrained TypeVar in a union

This is a regression test for an issue that surfaced in the primer report of an early version of
<https://github.com/astral-sh/ruff/pull/19811>, where we failed to solve the `TypeVar` here due to
the fact that it only appears in the function's type annotations as part of a union:

```py
from typing import TypeVar

T = TypeVar("T", str, bytes)

def NamedTemporaryFile(suffix: T | None, prefix: T | None) -> None:
    return None

def f(x: str):
    NamedTemporaryFile(prefix=x, suffix=".tar.gz")  # Fine
```

## Nested functions see typevars bound in outer function

```py
from typing import TypeVar, overload

T = TypeVar("T")
S = TypeVar("S")

def outer(t: T) -> None:
    def inner(t: T) -> None: ...

    inner(t)

@overload
def overloaded_outer() -> None: ...
@overload
def overloaded_outer(t: T) -> None: ...
def overloaded_outer(t: T | None = None) -> None:
    def inner(t: T) -> None: ...

    if t is not None:
        inner(t)

def outer(t: T) -> None:
    def inner(inner_t: T, s: S) -> tuple[T, S]:
        return inner_t, s
    reveal_type(inner(t, 1))  # revealed: tuple[T@outer, Literal[1]]

    inner("wrong", 1)  # error: [invalid-argument-type]
```

## Unpacking a TypeVar

We can infer precise heterogeneous types from the result of an unpacking operation applied to a type
variable if the type variable's upper bound is a type with a precise tuple spec:

```py
from dataclasses import dataclass
from typing import NamedTuple, Final, TypeVar, Generic

T = TypeVar("T", bound=tuple[int, str])

def f(x: T) -> T:
    a, b = x
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: str
    return x

@dataclass
class Team(Generic[T]):
    employees: list[T]

def x(team: Team[T]) -> Team[T]:
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

## `~T` is never assignable to `T`

```py
from typing import TypeVar
from ty_extensions import Not

T = TypeVar("T")

def f(x: T, y: Not[T]) -> T:
    x = y  # error: [invalid-assignment]
    y = x  # error: [invalid-assignment]
    return x
```

## Prefer exact matches for constrained typevars

```py
from typing import TypeVar

class Base: ...
class Sub(Base): ...

# We solve to `Sub`, regardless of the order of constraints.
T = TypeVar("T", Base, Sub)
T2 = TypeVar("T2", Sub, Base)

def f(x: T) -> list[T]:
    return [x]

def f2(x: T2) -> list[T2]:
    return [x]

x: list[Sub] = f(Sub())
reveal_type(x)  # revealed: list[Sub]

y: list[Sub] = f2(Sub())
reveal_type(y)  # revealed: list[Sub]
```
