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

class ExplicitlyImplements(CanIndex[T]):
    def __getitem__(self, index: int, /) -> T:
        raise NotImplementedError

class SubProtocol(CanIndex[T], Protocol): ...

def takes_in_list(x: list[T]) -> list[T]:
    return x

def takes_in_protocol(x: CanIndex[T]) -> T:
    return x[0]

def deep_list(x: list[str]) -> None:
    reveal_type(takes_in_list(x))  # revealed: list[str]
    reveal_type(takes_in_protocol(x))  # revealed: str

def deeper_list(x: list[set[str]]) -> None:
    reveal_type(takes_in_list(x))  # revealed: list[set[str]]
    reveal_type(takes_in_protocol(x))  # revealed: set[str]

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
reveal_type(takes_in_protocol(Sub()))  # revealed: int

reveal_type(takes_in_list(GenericSub[str]()))  # revealed: list[str]
reveal_type(takes_in_protocol(GenericSub[str]()))  # revealed: str

class ExplicitSub(ExplicitlyImplements[int]): ...
class ExplicitGenericSub(ExplicitlyImplements[T]): ...

reveal_type(takes_in_protocol(ExplicitSub()))  # revealed: int
reveal_type(takes_in_protocol(ExplicitGenericSub[str]()))  # revealed: str
```

An overload is not a match if it requires a type-variable solution that violates the declared bound.
Here, the first overload would require `T_str` to be `int`, which does not satisfy the bound `str`,
so the second overload is selected.

```py
from collections.abc import Iterable
from typing import TypeVar, overload

T_str = TypeVar("T_str", bound=str)

@overload
def pick(x: Iterable[T_str]) -> T_str: ...
@overload
def pick(x: Iterable[int]) -> bool: ...
def pick(x: object) -> str | bool:
    raise NotImplementedError

reveal_type(pick([1]))  # revealed: bool
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

```py
from typing import TypeVar

T = TypeVar("T", bound=int)

def f(x: T) -> T:
    return x

reveal_type(f(1))  # revealed: Literal[1]
reveal_type(f(True))  # revealed: Literal[True]
# snapshot: invalid-argument-type
reveal_type(f("string"))  # revealed: Unknown
```

```snapshot
error[invalid-argument-type]: Argument to function `f` is incorrect
  --> src/mdtest_snippet.py:11:15
   |
11 | reveal_type(f("string"))  # revealed: Unknown
   |               ^^^^^^^^ Argument type `Literal["string"]` does not satisfy upper bound `int` of type variable `T`
   |
info: Type variable defined here
 --> src/mdtest_snippet.py:3:1
  |
3 | T = TypeVar("T", bound=int)
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
```

A bound can also be a union of protocols. If inference produces a union for the type variable, each
member must satisfy at least one protocol in the bound. `int` supports ordering, but `None` does
not, so `None | int` is invalid.

```py
from collections.abc import Iterable
from typing import Any, Protocol, TypeVar

class SupportsLT(Protocol):
    def __lt__(self, other: Any, /) -> object: ...

class SupportsGT(Protocol):
    def __gt__(self, other: Any, /) -> object: ...

ComparableT = TypeVar("ComparableT", bound=SupportsLT | SupportsGT)

def consume_comparable(values: Iterable[ComparableT]) -> None: ...

consume_comparable([None, 2])  # error: [invalid-argument-type]
```

## Inferring a constrained typevar

```py
from typing import TypeVar

T = TypeVar("T", int, None)

def f(x: T) -> T:
    return x

reveal_type(f(1))  # revealed: int
reveal_type(f(True))  # revealed: int
reveal_type(f(None))  # revealed: None
# snapshot: invalid-argument-type
reveal_type(f("string"))  # revealed: Unknown
```

```snapshot
error[invalid-argument-type]: Argument to function `f` is incorrect
  --> src/mdtest_snippet.py:12:15
   |
12 | reveal_type(f("string"))  # revealed: Unknown
   |               ^^^^^^^^ Argument type `Literal["string"]` does not satisfy constraints (`int`, `None`) of type variable `T`
   |
info: Type variable defined here
 --> src/mdtest_snippet.py:3:1
  |
3 | T = TypeVar("T", int, None)
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
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
    return t1 + t2

S = TypeVar("S", int, float)

def chained_constrained_types(t1: S, t2: S, t3: S) -> S:
    return (t1 + t2) * t3

def typevar_times_literal(t: S) -> S:
    return t * 2

def literal_times_typevar(t: S) -> S:
    return 2 * t

def negate_typevar(t: S) -> S:
    return -t

def positive_typevar(t: S) -> S:
    return +t
```

Narrowing should preserve the constrained typevar identity so the narrowed value remains assignable
to the function's return type:

```py
from typing import TypeVar

class P: ...
class Q: ...

NarrowedT = TypeVar("NarrowedT", P, Q)

def return_narrowed_typevar(x: NarrowedT) -> NarrowedT:
    if isinstance(x, P):
        return x
    return x
```

Unary operations that are not supported by all constraints should error:

```py
from typing import TypeVar

U = TypeVar("U", int, float)

def invert_typevar(t: U) -> int:
    # error: [unsupported-operator] "Unary operator `~` is not supported for object of type `U@invert_typevar`"
    return ~t
```

This is _not_ the same as a union type, because of this additional constraint that the two
occurrences have the same type. In `unions_are_different`, `t1` and `t2` might have different types,
and an `int` and a `str` cannot be added together:

```py
def unions_are_different(t1: int | str, t2: int | str) -> int | str:
    # error: [unsupported-operator] "Operator `+` is not supported between two objects of type `int | str`"
    return t1 + t2
```

## Constraints containing `Any`

A heterogeneous collection can infer a union of tuple types. If every member of that union is
compatible with `tuple[Any, ...]`, a constrained type variable can use that constraint.

```py
from collections.abc import Callable, Iterable
from typing import Any, TypeVar

Row = TypeVar("Row", list[Any], tuple[Any, ...])

class Dense: ...
class Sparse: ...

def consume(rows: Iterable[Row]) -> Row:
    raise NotImplementedError

reveal_type(consume([(1.0, Dense()), (0.0, Sparse())]))  # revealed: tuple[Any, ...]

def callback(row: tuple[int, ...]) -> None: ...
def consume_callback(callback: Callable[[Row], None]) -> Row:
    raise NotImplementedError

reveal_type(consume_callback(callback))  # revealed: tuple[Any, ...]
```

## Gradual constraints can obscure a more specific constraint

A gradual constraint that is compatible with a concrete argument can be selected before a more
specific constraint. This makes inference depend on the order in which the constraints are declared.

```py
from typing import Any, TypeVar

class Row(tuple[Any, ...]):
    def asDict(self) -> dict[str, Any]:
        raise NotImplementedError

GradualFirst = TypeVar("GradualFirst", list[Any], tuple[Any, ...], Row)
RowFirst = TypeVar("RowFirst", Row, tuple[Any, ...], list[Any])

def gradual_first(row: GradualFirst) -> GradualFirst:
    return row

def row_first(row: RowFirst) -> RowFirst:
    return row

gradual = gradual_first(Row())
# TODO: revealed: Row
reveal_type(gradual)  # revealed: tuple[Any, ...]
# error: [unresolved-attribute] "Object of type `tuple[Any, ...]` has no attribute `asDict`"
gradual.asDict()

specific = row_first(Row())
reveal_type(specific)  # revealed: Row
specific.asDict()
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

## Upper-bound inference preserves intersection order

When a typevar occurs contravariantly, argument matching can provide only upper bounds for its
solution. Multiple upper bounds are intersected in the order in which they occur at the call site.

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

## Recursive generic calls

Recursive occurrences of a generic function should be treated as fresh generic callable occurrences.
The recursive call's typevars are inferable at the call site, even though the function body's own
typevars are non-inferable.

```py
from typing import TypeVar

T = TypeVar("T")
A = TypeVar("A")
B = TypeVar("B")

def recursive_identity(t: T) -> T:
    reveal_type(recursive_identity(t))  # revealed: T@recursive_identity
    return t

def pair(a: A, b: B) -> tuple[A, B]:
    return (a, b)

def recursive_pair(t: T) -> T:
    reveal_type(pair(recursive_pair(t), recursive_pair(1)))  # revealed: tuple[T@recursive_pair, Literal[1]]
    return t
```

## Union parameter inference

When one of the parameters is a union, we attempt to find the smallest specialization that satisfies
all of the constraints.

```py
from typing import TypeVar

T = TypeVar("T")

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
T_str2 = TypeVar("T_str2", bound=str)

def accepts_t_or_list_of_t(x: T_str2 | list[T_str2]) -> T_str2:
    raise NotImplementedError

reveal_type(accepts_t_or_list_of_t("a"))  # revealed: Literal["a"]
# error: [invalid-argument-type] "Argument type `Literal[1]` does not satisfy upper bound `str` of type variable `T_str2`"
reveal_type(accepts_t_or_list_of_t(1))  # revealed: Unknown

def _(list_ofstr: list[str], list_of_int: list[int]):
    reveal_type(accepts_t_or_list_of_t(list_ofstr))  # revealed: str

    # TODO: the error message here could be improved by referring to the second union element
    # error: [invalid-argument-type] "Argument type `list[int]` does not satisfy upper bound `str` of type variable `T_str2`"
    reveal_type(accepts_t_or_list_of_t(list_of_int))  # revealed: Unknown
```

A union argument must not widen a bounded type variable with an incompatible union element:

```py
class MyClass: ...

T_bounded = TypeVar("T_bounded", bound=MyClass)

def accepts_instance_or_int(instance: T_bounded, x: T_bounded | int) -> T_bounded:
    return instance

def _(x: int | None, valid: MyClass | int) -> MyClass:
    # error: [invalid-argument-type] "Argument type `None` does not satisfy upper bound `MyClass` of type variable `T_bounded`"
    result = accepts_instance_or_int(MyClass(), x)
    reveal_type(result)  # revealed: MyClass
    reveal_type(accepts_instance_or_int(MyClass(), valid))  # revealed: MyClass
    return result
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

## Inference from unions containing generic classes

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

## Attribute access on `Callable`-bounded TypeVars

```py
from typing import Any, Callable, Generic, TypeVar

F = TypeVar("F", bound=Callable[..., Any])

def my_decorator(f: F) -> None:
    # error: [unresolved-attribute]
    f.whatever
    # error: [unresolved-attribute]
    f.whatever = 1

class Box(Generic[F]):
    cls: type[F]

def specialized(box: Box[Callable[..., Any]]) -> None:
    # error: [unresolved-attribute]
    box.cls.whatever
```

## Attribute access on TypeVars bounded by `type[...]`

Regression test for <https://github.com/astral-sh/ty/issues/3782>.

```py
from typing import ClassVar, TypeVar
from typing_extensions import Self

class A:
    attr: ClassVar[str]
    current: ClassVar[Self]

    @classmethod
    def create(cls) -> Self:
        return cls()

class B:
    attr: ClassVar[int]

T = TypeVar("T", bound=type[A])

def single_bound(cls: T) -> None:
    reveal_type(cls.attr)  # revealed: str
    reveal_type(cls.current)  # revealed: T'instance@single_bound
    reveal_type(cls.create())  # revealed: T'instance@single_bound

U = TypeVar("U", bound=type[A] | type[B])

def union_bound(cls: U) -> None:
    reveal_type(cls.attr)  # revealed: str | int
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

## Prefer specific compatible constraints over union constraints

When multiple declared constraints are compatible with a lower bound, we prefer the most specific
one. This does not depend on the order in which the constraints were declared.

```py
from typing import TypeVar

BroadFirst = TypeVar("BroadFirst", str | bytes, str, bytes)
NarrowFirst = TypeVar("NarrowFirst", str, bytes, str | bytes)

def broad_first(value: BroadFirst) -> BroadFirst:
    return value

def narrow_first(value: NarrowFirst) -> NarrowFirst:
    return value

def check(value: str) -> None:
    reveal_type(broad_first(value))  # revealed: str
    reveal_type(narrow_first(value))  # revealed: str
```

## Prefer general constraints for upper-bound-only inference

When inference provides only an upper bound, we prefer the most general compatible declared
constraint. This also does not depend on declaration order.

```py
from typing import Callable, TypeVar

NarrowFirst = TypeVar("NarrowFirst", int, object)
BroadFirst = TypeVar("BroadFirst", object, int)

def narrow_first(callback: Callable[[NarrowFirst], None]) -> NarrowFirst:
    raise NotImplementedError

def broad_first(callback: Callable[[BroadFirst], None]) -> BroadFirst:
    raise NotImplementedError

def accepts_object(value: object) -> None: ...

reveal_type(narrow_first(accepts_object))  # revealed: object
reveal_type(broad_first(accepts_object))  # revealed: object
```

## Ambiguous constrained TypeVar inference from `Any`

A gradual argument alone provides no evidence for choosing between multiple compatible constraints.
We currently fall back to `Unknown` rather than choosing an arbitrary concrete constraint. Ideally,
we would preserve `Any` instead.

```py
from typing import Any, TypeVar

T = TypeVar("T", int, int | list[int])

def identity(value: T) -> T:
    return value

def choose(left: T, right: T) -> T:
    return left

def caller(value: Any) -> None:
    reveal_type(identity(value))  # revealed: Any
    # TODO: revealed: Any
    reveal_type(choose(value, 1))  # revealed: int

def list_caller(value: list[Any]) -> None:
    reveal_type(identity(value))  # revealed: int | list[int]
    reveal_type(choose(value, 1))  # revealed: int | list[int]
    reveal_type(choose(value, [1]))  # revealed: int | list[int]
```

## Ambiguous constrained TypeVar inference from a gradual callable return

Constraint-set-native inference also preserves gradual evidence nested inside a callable. As above,
we currently fall back to `Unknown` when that evidence matches multiple constraints.

```py
from typing import Any, Callable, TypeVar

T = TypeVar("T", int, int | list[int])

def call(callback: Callable[[], T]) -> T:
    return callback()

def callback() -> Any:
    return 1

reveal_type(call(callback))  # revealed: Any
```

## Bounded TypeVar with callable parameter

When a bounded TypeVar appears in a `Callable` parameter's return type, the inferred type should be
the actual type from the call, not the TypeVar's upper bound.

See: <https://github.com/astral-sh/ty/issues/2292>

```py
from typing import Callable, TypeVar

class Base:
    pass

class Derived(Base):
    attr: int

T = TypeVar("T", bound=Base)

def takes_factory(factory: Callable[[], T]) -> T:
    return factory()

# Passing a class as a factory: should infer Derived, not Base
result = takes_factory(Derived)
reveal_type(result)  # revealed: Derived

# Accessing an attribute that only exists on Derived should work
print(result.attr)  # No error
```

## Callable instances

Generic parameters can be inferred from the `__call__` method of a class instance.

```py
from typing import Callable, TypeVar

R = TypeVar("R")

def call(callable: Callable[[], R]) -> R:
    return callable()

class MyCallable:
    def __call__(self) -> int:
        return 1

reveal_type(call(MyCallable()))  # revealed: int
```

## Passing a constrained TypeVar to a function expecting a compatible constrained TypeVar

A constrained TypeVar should be assignable to a different constrained TypeVar if each constraint of
the actual TypeVar is equivalent to at least one constraint of the formal TypeVar. This commonly
arises when wrapping functions from external packages that define private TypeVars with the same
constraints.

See: <https://github.com/astral-sh/ty/issues/2728>

```py
from typing import TypeVar

T = TypeVar("T", int, str)
S = TypeVar("S", int, str)

def callee(x: T) -> T:
    return x

def caller(x: S) -> S:
    return callee(x)

reveal_type(caller(1))  # revealed: int
reveal_type(caller("hello"))  # revealed: str
```

A constrained TypeVar with a subset of constraints is also compatible:

```py
from typing import TypeVar

Wide = TypeVar("Wide", int, str, bytes)
Narrow = TypeVar("Narrow", int, str)

def wide(x: Wide) -> Wide:
    return x

def narrow(x: Narrow) -> Narrow:
    return wide(x)

reveal_type(narrow(1))  # revealed: int
reveal_type(narrow("hello"))  # revealed: str
```

## Incompatible constraint sets

But a constrained TypeVar with constraints not satisfied by the formal TypeVar should still error:

```py
from typing import TypeVar

T = TypeVar("T", int, str)
U = TypeVar("U", int, bytes)

def target(x: T) -> T:
    return x

def source(x: U) -> U:
    return target(x)  # error: [invalid-argument-type]
```

## Constraint equivalence

We require equivalence rather than mere assignability when matching constraints. Constrained
TypeVars allow narrowing via `isinstance` checks in the function body, so a constraint that is a
strict subtype would be unsound. For example, a function constrained to `(int, str)` may narrow `T`
to `int` and return `int(x)`, which would violate a caller's `bool` constraint:

```py
from typing import TypeVar

T = TypeVar("T", int, str)
S = TypeVar("S", bool, str)

def f(x: T) -> T:
    return x

def g(x: S) -> S:
    return f(x)  # error: [invalid-argument-type]
```

## Inferring typevars in iterable parameters from literal string and bytes arguments

```py
from typing import Iterable, TypeVar
from typing_extensions import LiteralString

FlatT = TypeVar("FlatT")

def flatten(*iterables: Iterable[FlatT]) -> list[FlatT]:
    return [x for iterable in iterables for x in iterable]

def flatten_covariant(*iterables: Iterable[FlatT]) -> tuple[FlatT, ...]:
    return tuple(x for iterable in iterables for x in iterable)

# TODO: revealed: list[LiteralString | int]
reveal_type(flatten("abc", (1, 2, 3)))  # revealed: list[str | int]
# TODO: revealed: tuple[LiteralString | Literal[1, 2, 3], ...]
reveal_type(flatten_covariant("abc", (1, 2, 3)))  # revealed: tuple[str | Literal[1, 2, 3], ...]

def literal_string_case(literal_string: LiteralString):
    # TODO: revealed: list[LiteralString | int]
    reveal_type(flatten(literal_string, (1, 2, 3)))  # revealed: list[str | int]

def literal_string_case(string: str):
    reveal_type(flatten(string, (1, 2, 3)))  # revealed: list[str | int]

reveal_type(flatten(b"abc"))  # revealed: list[int]
reveal_type(flatten(b"abc", ("x",)))  # revealed: list[int | str]
# TODO: we could have `Literal[97, 98, 99]` instead of `int` in the next two lines
reveal_type(flatten_covariant(b"abc"))  # revealed: tuple[int, ...]
reveal_type(flatten_covariant(b"abc", ("x",)))  # revealed: tuple[int | Literal["x"], ...]
```

## Inferring typevars in intersections (formal type position)

```py
from typing import TypeVar, Iterable
from ty_extensions import Intersection

T = TypeVar("T")

class Foo: ...

def foo(x: Intersection[Iterable[T], Foo]) -> T:
    return next(iter(x))

class Bar(list[int], Foo): ...

reveal_type(foo(Bar()))  # revealed: int
```

## Inferring typevars in intersections (actual type position)

```py
from typing import TypeVar, Sequence, Iterable

T = TypeVar("T")

def first(iterable: Iterable[T]) -> T:
    return next(iter(iterable))

def narrowed_via_isinstance(x: Sequence[str] | int):
    if isinstance(x, int):
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: Sequence[str] & ~int
        reveal_type(first(x))  # revealed: str

def narrowed_via_truthiness(y: list[str]):
    if y:
        reveal_type(y)  # revealed: list[str] & ~AlwaysFalsy
        reveal_type(first(y))  # revealed: str
```

## Inferring typevars in intersections (actual type position, multiple positive types)

When an actual intersection has multiple specializations of the same covariant generic class, we
combine the type arguments before inferring a bounded typevar:

```py
from typing import Sequence, TypeVar
from ty_extensions import Intersection

class Base: ...
class Sub1(Base): ...
class Sub2(Base): ...
class Unrelated1: ...
class Unrelated2: ...

T = TypeVar("T", bound=Base)

def first(x: Sequence[T]) -> T:
    return x[0]

# Both positive elements satisfy the bound.
def _(x: Intersection[Sequence[Sub1], Sequence[Sub2]]) -> None:
    reveal_type(first(x))  # revealed: Sub1 & Sub2

# An intersection is a subtype of the bound if one of its positive elements is a subtype of the
# bound.
def _(x: Intersection[Sequence[Sub1], Sequence[Unrelated1]]) -> None:
    reveal_type(first(x))  # revealed: Sub1 & Unrelated1

# Additional positive elements are preserved in the inferred type.
def _(x: Intersection[Sequence[Sub1], Sequence[Sub2], Sequence[Unrelated1]]) -> None:
    reveal_type(first(x))  # revealed: Sub1 & Sub2 & Unrelated1

# An intersection with two positive elements, neither of which produces a valid specialization.
def _(x: Intersection[Sequence[Unrelated1], Sequence[Unrelated2]]) -> None:
    # error: [invalid-argument-type] "Argument to function `first` is incorrect: Argument type `Unrelated1 & Unrelated2` does not satisfy upper bound `Base` of type variable `T`"
    reveal_type(first(x))  # revealed: Unknown

Constrained = TypeVar("Constrained", Sub1, Sub2)

def first_constrained(x: Sequence[Constrained]) -> Constrained:
    return x[0]

def _(x: Intersection[Sequence[Unrelated1], Sequence[Unrelated2]]) -> None:
    # error: [invalid-argument-type] "Argument to function `first_constrained` is incorrect: Argument type `Unrelated1 & Unrelated2` does not satisfy constraints (`Sub1`, `Sub2`) of type variable `Constrained`"
    reveal_type(first_constrained(x))  # revealed: Unknown
```
