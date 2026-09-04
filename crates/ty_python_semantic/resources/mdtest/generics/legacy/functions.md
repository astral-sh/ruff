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
reveal_type(f(1.0))  # revealed: float*
reveal_type(f(True))  # revealed: Literal[True]
reveal_type(f("string"))  # revealed: Literal["string"]
```

## Simple generic calls

Arguments to a generic function determine its return type without bypassing ordinary argument
checking. Gradual arguments can also affect the inferred return type.

```py
from typing import Any, TypeVar

T = TypeVar("T")
U = TypeVar("U")

def one_path(value: T, other: int) -> T:
    return value

# error: [invalid-argument-type]
reveal_type(one_path("value", "bad"))  # revealed: Literal["value"]

def with_gradual(value: T, other: U) -> tuple[T, U]:
    return value, other

def _(value: Any) -> None:
    reveal_type(with_gradual("value", value))  # revealed: tuple[Literal["value"], Any]
```

## Inferring “deep” generic parameter types

The matching up of call arguments and discovery of constraints on typevars can be a recursive
process for arbitrarily-nested generic classes and protocols in parameters.

TODO: Note that we can currently only infer a specialization for a generic protocol when the
argument _explicitly_ implements the protocol by listing it as a base class.

```py
from typing import Protocol, TypeVar

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)

class CanIndex(Protocol[T_co]):
    def __getitem__(self, index: int, /) -> T_co: ...

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

def takes_in_type_of_list(x: type[list[T]]) -> T:
    raise NotImplementedError

reveal_type(takes_in_type_of_list(list[int]))  # revealed: int
```

This also works when passing in arguments that are subclasses of the parameter type.

```py
class Sub(list[int]): ...
class GenericSub(list[T]): ...

reveal_type(takes_in_list(Sub()))  # revealed: list[int]
reveal_type(takes_in_protocol(Sub()))  # revealed: int

reveal_type(takes_in_list(GenericSub[str]()))  # revealed: list[str]
reveal_type(takes_in_protocol(GenericSub[str]()))  # revealed: str

reveal_type(takes_in_type_of_list(Sub))  # revealed: int
reveal_type(takes_in_type_of_list(GenericSub[str]))  # revealed: str

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

## Inferring generic typed-dictionary parameters

A type variable that appears only inside a typed dictionary still makes the function generic, so
specialized typed dictionaries can be passed to it.

```py
from typing import Generic, TypeVar, TypedDict

T = TypeVar("T")

class Item(TypedDict, Generic[T]):
    value: T

def accept(value: Item[T]) -> None: ...

item: Item[int] = {"value": 1}

reveal_type(accept)  # revealed: def accept[T](value: Item[T]) -> None
accept(item)
```

## Inferring a class-object parameter through a generic factory

A factory can infer its type arguments from a specialized subclass of its class-object parameter.

```py
from typing import Generic, TypeVar

T = TypeVar("T")
U = TypeVar("U")

class Base(Generic[T, U]): ...
class Specialized(Base[int, str]): ...

def create(cls: type[Base[T, U]]) -> tuple[T, U]:
    raise NotImplementedError

reveal_type(create(Specialized))  # revealed: tuple[int, str]
```

## Inferring a class-object parameter through a generic method

A method can likewise infer a type argument from the specialized bases of its class-object
parameter.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Option(Generic[T]): ...
class StringOption(Option[str]): ...

class Options:
    def get_value_for(self, option: type[Option[T]]) -> T:
        raise NotImplementedError

reveal_type(Options().get_value_for(StringOption))  # revealed: str
```

## Inferring a class-object parameter in a contravariant position

A class-object parameter inside a contravariant generic places an upper bound on its inferred type
argument. A more specific witness should determine the result, including when the type variable has
its own declared upper bound.

```py
from typing import Generic, TypeVar

T = TypeVar("T")
StrT = TypeVar("StrT", bound=str)
T_co = TypeVar("T_co", covariant=True)
T_contra = TypeVar("T_contra", contravariant=True)

class Covariant(Generic[T_co]): ...
class Sink(Generic[T_contra]): ...

def infer(sink: Sink[type[Covariant[T]]], witness: T) -> T:
    return witness

def infer_bounded(sink: Sink[type[Covariant[StrT]]], witness: StrT) -> StrT:
    return witness

def _(sink: Sink[type[Covariant[object]]]) -> None:
    reveal_type(infer(sink, 1))  # revealed: Literal[1]
    reveal_type(infer_bounded(sink, "ok"))  # revealed: Literal["ok"]
```

## Inferring a class-object parameter for a final generic class

A final generic class has no subclasses, so its class-object parameter is an exact generic alias.
Its type arguments should still participate in inference.

```py
from typing import Generic, TypeVar, final

T = TypeVar("T")

@final
class Final(Generic[T]): ...

def infer(cls: type[Final[T]]) -> T:
    raise NotImplementedError

reveal_type(infer(Final[int]))  # revealed: int
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

## Inferring tuple parameter types from unions

```toml
[environment]
python-version = "3.11"
```

Every member of a union argument contributes to the inferred element type of a homogeneous tuple
parameter. Different tuple lengths do not prevent inference, and an empty tuple contributes no
element types.

```py
from typing import TypeVar

class A: ...
class B: ...
class C: ...
class D: ...

T = TypeVar("T")

def elements(values: tuple[T, ...]) -> tuple[T, ...]:
    return values

def _(
    same: tuple[A, A] | tuple[A, A, A],
    mixed: tuple[A] | tuple[B, B],
    possibly_empty: tuple[()] | tuple[A, A],
):
    reveal_type(elements(same))  # revealed: tuple[A, ...]
    reveal_type(elements(mixed))  # revealed: tuple[A | B, ...]
    reveal_type(elements(possibly_empty))  # revealed: tuple[A, ...]
```

Fixed-length and mixed tuples infer type parameters from their corresponding element positions.

```py
U = TypeVar("U")

def swap(values: tuple[U, T]) -> tuple[T, U]:
    return values[1], values[0]

def _(pairs: tuple[A, B] | tuple[C, D]):
    reveal_type(swap(pairs))  # revealed: tuple[B | D, A | C]

def tail(values: tuple[A, *tuple[T, ...]]) -> tuple[T, ...]:
    return values[1:]

def _(tails: tuple[A, B] | tuple[A, C, C]):
    reveal_type(tail(tails))  # revealed: tuple[B | C, ...]
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
info: Type variable defined here
 --> src/mdtest_snippet.py:3:1
  |
3 | T = TypeVar("T", bound=int)
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^
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
info: Type variable defined here
 --> src/mdtest_snippet.py:3:1
  |
3 | T = TypeVar("T", int, None)
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^
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

## Gradual invariant protocol members

When the same inferred type variable appears in multiple invariant protocol members, fully static
member types must agree on one exact specialization. Gradual members remain conservative
alternatives because their equality cannot justify a transitive sequent proof.

```py
from typing import Any, Generic, Protocol, TypeVar

T = TypeVar("T")
U = TypeVar("U")

class Pair(Protocol[T]):
    first: T
    second: T

class GradualPair(Generic[U]):
    first: tuple[U, Any]
    second: tuple[U, int]

def infer_pair(value: Pair[T]) -> T:
    raise NotImplementedError

def check_pair(value: GradualPair[U]) -> None:
    # TODO: error: [invalid-argument-type] "Argument to function `infer_pair` is incorrect"
    reveal_type(infer_pair(value))  # revealed: tuple[U@check_pair, Any] | tuple[U@check_pair, int]
```

## Prefer specific compatible constraints over gradual constraints

A gradual constraint can be compatible with a concrete argument and a more specific declared
constraint. We prefer the more specific constraint regardless of declaration order.

```py
from typing import Any, TypeVar

class Row(tuple[Any, ...]):
    def asDict(self) -> dict[str, Any]:
        raise NotImplementedError

GradualFirst = TypeVar("GradualFirst", list[Any], tuple[Any, ...], Row)
RowFirst = TypeVar("RowFirst", Row, tuple[Any, ...], list[Any])
AnyFirst = TypeVar("AnyFirst", Any, int)
IntFirst = TypeVar("IntFirst", int, Any)

def gradual_first(row: GradualFirst) -> GradualFirst:
    return row

def row_first(row: RowFirst) -> RowFirst:
    return row

def any_first(value: AnyFirst) -> AnyFirst:
    return value

def int_first(value: IntFirst) -> IntFirst:
    return value

gradual = gradual_first(Row())
reveal_type(gradual)  # revealed: Row
gradual.asDict()

specific = row_first(Row())
reveal_type(specific)  # revealed: Row
specific.asDict()

reveal_type(any_first(1))  # revealed: int
reveal_type(int_first(1))  # revealed: int
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

# error: [dynamic-function-decorator-return]
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

## Attribute access on TypeVars constrained to instances and class objects

A constrained type variable can contain both ordinary instances and class objects. Accessing a
shared attribute must inspect each constraint without treating the entire type variable as a class.

```py
from typing import TypeVar

class Instance:
    @staticmethod
    def keys() -> list[str]:
        return []

class ClassObject:
    @staticmethod
    def keys() -> list[str]:
        return []

T = TypeVar("T", Instance, type[ClassObject])

def read(value: T) -> list[str]:
    return value.keys()
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

## Gradual bounds in generic union members

A gradual bound does not prevent inference from an invariant union member: `str` satisfies `Any`,
and `list[str]` satisfies `list[Any]`.

```py
from typing import Any, TypeVar

class Other: ...

T = TypeVar("T", bound=Any)

def infer_any_bound(value: list[T] | Other) -> T:
    raise NotImplementedError

ListBoundT = TypeVar("ListBoundT", bound=list[Any])

def infer_list_bound(value: list[ListBoundT] | Other) -> ListBoundT:
    raise NotImplementedError

reveal_type(infer_any_bound(list[str]()))  # revealed: str
reveal_type(infer_list_bound(list[list[str]]()))  # revealed: list[str]
```

## Invalid bounds in generic union members

An argument that violates a type variable's bound is rejected even when another union member is not
disjoint from the argument. `list[object]` and `Other` can have a common subclass, but
`list[object]` is not assignable to `Other`, and `object` does not satisfy the bound of `T`.

```py
from typing import TypeVar

class Other: ...

T = TypeVar("T", bound=str)

def accept(value: list[T] | Other) -> None:
    pass

accept([])
accept(["valid"])
accept(Other())

accept([object()])  # error: [invalid-argument-type] "does not satisfy upper bound `str`"
accept([1])  # error: [invalid-argument-type] "does not satisfy upper bound `str`"
```

## Disjoint generic union members

The `list[T]` member cannot match a string or `None`. Inference through the remaining `T` member
rejects `None`, which satisfies neither of its constraints.

```py
from typing import TypeVar

T = TypeVar("T", str, bytes)

def accept(value: T | list[T]) -> None:
    pass

def _(value: str | None):
    accept(value)  # error: [invalid-argument-type] "does not satisfy constraints"
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

The `Unknown` returned by a lambda without declared parameter types is also gradual evidence:

```py
lambda_identity = lambda value: value

def lambda_caller(value: Any) -> None:
    reveal_type(identity(lambda_identity(value)))  # revealed: Unknown
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

## Callable return union order does not affect inference

```py
from typing import Callable, Generic, TypeVar

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)

class Box(Generic[T_co]): ...

def ensure_tuple(func: Callable[[], tuple[T, ...] | T]) -> tuple[T, ...]:
    raise NotImplementedError

def ensure_tuple_reversed(func: Callable[[], T | tuple[T, ...]]) -> tuple[T, ...]:
    raise NotImplementedError

def ensure_box(func: Callable[[], Box[T] | T]) -> Box[T]:
    raise NotImplementedError

def ensure_box_reversed(func: Callable[[], T | Box[T]]) -> Box[T]:
    raise NotImplementedError

def check(
    scalar_first: Callable[[], str | tuple[str, ...]],
    tuple_first: Callable[[], tuple[str, ...] | str],
    nested_member_first: Callable[[], Box[str] | tuple[Box[str], ...]],
    nested_tuple_first: Callable[[], tuple[Box[str], ...] | Box[str]],
    box_scalar_first: Callable[[], str | Box[str]],
    box_first: Callable[[], Box[str] | str],
) -> None:
    reveal_type(ensure_tuple(scalar_first))  # revealed: tuple[str, ...]
    reveal_type(ensure_tuple(tuple_first))  # revealed: tuple[str, ...]
    reveal_type(ensure_tuple_reversed(scalar_first))  # revealed: tuple[str, ...]
    reveal_type(ensure_tuple_reversed(tuple_first))  # revealed: tuple[str, ...]
    reveal_type(ensure_tuple(nested_member_first))  # revealed: tuple[Box[str], ...]
    reveal_type(ensure_tuple(nested_tuple_first))  # revealed: tuple[Box[str], ...]
    reveal_type(ensure_tuple_reversed(nested_member_first))  # revealed: tuple[Box[str], ...]
    reveal_type(ensure_tuple_reversed(nested_tuple_first))  # revealed: tuple[Box[str], ...]
    reveal_type(ensure_box(box_scalar_first))  # revealed: Box[str]
    reveal_type(ensure_box(box_first))  # revealed: Box[str]
    reveal_type(ensure_box_reversed(box_scalar_first))  # revealed: Box[str]
    reveal_type(ensure_box_reversed(box_first))  # revealed: Box[str]
```

## Gradual container constraints preserve inference evidence

`Collection` inherits from `Container[Any]`, so inferring a type variable from a collection passed
to a contravariant `Container` must preserve the gradual constraint.

```py
from collections.abc import Container
from typing import Any, TypeVar

T = TypeVar("T")

def value(items: Container[T]) -> T:
    raise NotImplementedError

items: list[str] = []
reveal_type(value(items))  # revealed: Any
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

## Redundant callback bounds preserve constrained type-variable relationships

A contravariant callback can contribute both another constrained type variable and a redundant
`object` upper bound. The inferred result must retain the other type variable in either callback
order.

```py
from collections.abc import Callable
from typing import TypeVar

T = TypeVar("T", int, str)
S = TypeVar("S", int, str)

def select(first: Callable[[T], None], second: Callable[[T], None]) -> T:
    raise NotImplementedError

def forward_object(specific: Callable[[S], None], redundant: Callable[[object], None]) -> S:
    result = select(specific, redundant)
    reveal_type(result)  # revealed: S@forward_object
    return result

def forward_object_reversed(specific: Callable[[S], None], redundant: Callable[[object], None]) -> S:
    result = select(redundant, specific)
    reveal_type(result)  # revealed: S@forward_object_reversed
    return result
```

A union of the type variable's constraints is also a redundant upper bound, even though it is not
`object`.

```py
def forward_union(specific: Callable[[S], None], redundant: Callable[[int | str], None]) -> S:
    result = select(specific, redundant)
    reveal_type(result)  # revealed: S@forward_union
    return result

def forward_union_reversed(specific: Callable[[S], None], redundant: Callable[[int | str], None]) -> S:
    result = select(redundant, specific)
    reveal_type(result)  # revealed: S@forward_union_reversed
    return result
```

The same relationship must survive a redundant, non-`object` nominal superclass shared by both
constraints.

```py
class Base: ...
class Left(Base): ...
class Right(Base): ...

TNominal = TypeVar("TNominal", Left, Right)
SNominal = TypeVar("SNominal", Left, Right)

def select_nominal(first: Callable[[TNominal], None], second: Callable[[TNominal], None]) -> TNominal:
    raise NotImplementedError

def forward_nominal(specific: Callable[[SNominal], None], redundant: Callable[[Base], None]) -> SNominal:
    result = select_nominal(specific, redundant)
    reveal_type(result)  # revealed: SNominal@forward_nominal
    return result

def forward_nominal_reversed(specific: Callable[[SNominal], None], redundant: Callable[[Base], None]) -> SNominal:
    result = select_nominal(redundant, specific)
    reveal_type(result)  # revealed: SNominal@forward_nominal_reversed
    return result
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

When an actual intersection provides multiple valid specializations of a generic call, inference
keeps those paths separate and intersects the instantiated return types:

```py
from typing import Generic, Sequence, TypeVar
from ty_extensions import Intersection

class Base: ...
class Sub1(Base): ...
class SuperclassOfSub2(Base): ...
class Sub2(SuperclassOfSub2): ...
class SubclassOfSub2(Sub2): ...
class Unrelated1: ...
class Unrelated2: ...

T = TypeVar("T", bound=Base)

def first(x: Sequence[T]) -> T:
    return x[0]
```

Both positive elements satisfy the bound, so both specializations contribute to the return type:

```py
def _(x: Intersection[Sequence[Sub1], Sequence[Sub2]]) -> None:
    reveal_type(first(x))  # revealed: Sub1 & Sub2
```

Elements that do not satisfy the bound are ignored. Every valid specialization still contributes to
the return type:

```py
def _(x: Intersection[Sequence[Sub1], Sequence[Unrelated1]]) -> None:
    reveal_type(first(x))  # revealed: Sub1

def _(x: Intersection[Sequence[Sub1], Sequence[Sub2], Sequence[Unrelated1]]) -> None:
    reveal_type(first(x))  # revealed: Sub1 & Sub2
```

If neither positive element produces a valid specialization, the call reports a bound violation:

```py
def _(x: Intersection[Sequence[Unrelated1], Sequence[Unrelated2]]) -> None:
    # error: [invalid-argument-type] "Argument to function `first` is incorrect: Argument type `Unrelated1 & Unrelated2` does not satisfy upper bound `Base` of type variable `T`"
    reveal_type(first(x))  # revealed: Unknown
```

A constrained type variable must be solved to one of its declared constraints. Here, the call is
solved separately with `Constrained = Sub1` and `Constrained = Sub2`, and the return types are
intersected. `Constrained` itself is not solved to `Sub1 & Sub2`, which would not be a valid
declared constraint:

```py
Constrained = TypeVar("Constrained", Sub1, Sub2)

def first_constrained(x: Sequence[Constrained]) -> Constrained:
    return x[0]

def _(x: Intersection[Sequence[Sub1], Sequence[Sub2]]) -> None:
    reveal_type(first_constrained(x))  # revealed: Sub1 & Sub2
```

For `Sequence[SubclassOfSub2]`, `Constrained` must be solved to its declared `Sub2` constraint, not
to `SubclassOfSub2`. Intersecting this return type with `Sub1` therefore still gives `Sub1 & Sub2`:

```py
def _(x: Intersection[Sequence[Sub1], Sequence[SubclassOfSub2]]) -> None:
    reveal_type(first_constrained(x))  # revealed: Sub1 & Sub2
```

An element that does not match a declared constraint does not contribute to the return type:

```py
def _(x: Intersection[Sequence[Sub1], Sequence[Unrelated1]]) -> None:
    reveal_type(first_constrained(x))  # revealed: Sub1
```

If no element matches a declared constraint, the call reports a constraint violation:

```py
def _(x: Intersection[Sequence[Unrelated1], Sequence[Unrelated2]]) -> None:
    # error: [invalid-argument-type] "Argument to function `first_constrained` is incorrect: Argument type `Unrelated1 & Unrelated2` does not satisfy constraints (`Sub1`, `Sub2`) of type variable `Constrained`"
    reveal_type(first_constrained(x))  # revealed: Unknown
```

Incompatible invariant specializations are disjoint, so two valid positive elements must agree on
the constrained type:

```py
InvariantT = TypeVar("InvariantT")

class Box(Generic[InvariantT]):
    value: InvariantT

class Sub1Box(Box[Sub1]): ...
class OtherSub1Box(Box[Sub1]): ...
class Marker: ...

def unbox(x: Box[Constrained]) -> Constrained:
    raise NotImplementedError

def _(x: Intersection[Sub1Box, OtherSub1Box]) -> None:
    reveal_type(unbox(x))  # revealed: Sub1
```

A single matching element can still select either constraint:

```py
def _(x: Intersection[Box[Sub2], Marker]) -> None:
    reveal_type(unbox(x))  # revealed: Sub2
```

An unrelated intersection element does not make an invalid specialization of `Box` satisfy the
constraints:

```py
def _(x: Intersection[Box[Unrelated1], Marker]) -> None:
    # error: [invalid-argument-type] "Argument to function `unbox` is incorrect: Argument type `Unrelated1` does not satisfy constraints (`Sub1`, `Sub2`) of type variable `Constrained`"
    reveal_type(unbox(x))  # revealed: Unknown
```

Each argument independently selects a valid constraint, but invariance requires both arguments to
select the same one:

```py
def unbox_pair(x: Box[Constrained], y: Box[Constrained]) -> Constrained:
    raise NotImplementedError

def _(x: Intersection[Box[Sub1], Marker], y: Intersection[Box[Sub2], Marker]) -> None:
    # TODO: Both errors should report the incompatible constraints instead of expecting `Box[Sub1 | Sub2]`.
    # error: [invalid-argument-type] "Argument to function `unbox_pair` is incorrect: Expected `Box[Sub1 | Sub2]`, found `Box[Sub1] & Marker`"
    # error: [invalid-argument-type] "Argument to function `unbox_pair` is incorrect: Expected `Box[Sub1 | Sub2]`, found `Box[Sub2] & Marker`"
    reveal_type(unbox_pair(x, y))  # revealed: Sub1 | Sub2
```

For a contravariant sink, the selected constraint must be a subtype of the sink's element type. Each
valid specialization contributes its return type:

```py
ContravariantT = TypeVar("ContravariantT", contravariant=True)

class ConstrainedSink(Generic[ContravariantT]):
    def put(self, value: ContravariantT) -> None: ...

def sink_constrained(x: ConstrainedSink[Constrained]) -> Constrained:
    raise NotImplementedError

def _(x: Intersection[ConstrainedSink[Sub1], ConstrainedSink[Sub2]]) -> None:
    reveal_type(sink_constrained(x))  # revealed: Sub1 & Sub2
```

Contravariance allows a sink of a superclass to select the `Sub2` constraint:

```py
def _(x: Intersection[ConstrainedSink[Sub1], ConstrainedSink[SuperclassOfSub2]]) -> None:
    reveal_type(sink_constrained(x))  # revealed: Sub1 & Sub2

def _(x: Intersection[ConstrainedSink[SuperclassOfSub2], Marker]) -> None:
    reveal_type(sink_constrained(x))  # revealed: Sub2
```

A sink of a strict subclass cannot accept every `Sub2`, so it cannot select that constraint:

```py
def _(x: Intersection[ConstrainedSink[SubclassOfSub2], Marker]) -> None:
    # error: [invalid-argument-type] "Argument to function `sink_constrained` is incorrect: Argument type `SubclassOfSub2` does not satisfy constraints (`Sub1`, `Sub2`) of type variable `Constrained`"
    reveal_type(sink_constrained(x))  # revealed: Unknown
```

A valid specialization still contributes its return type when another element does not match a
declared constraint:

```py
def _(x: Intersection[ConstrainedSink[Sub1], ConstrainedSink[Unrelated1]]) -> None:
    reveal_type(sink_constrained(x))  # revealed: Sub1
```

If every element rules out the declared constraints, the call reports a constraint violation:

```py
def _(x: Intersection[ConstrainedSink[Unrelated1], ConstrainedSink[Unrelated2]]) -> None:
    # error: [invalid-argument-type] "Argument to function `sink_constrained` is incorrect: Argument type `Unrelated1 | Unrelated2` does not satisfy constraints (`Sub1`, `Sub2`) of type variable `Constrained`"
    reveal_type(sink_constrained(x))  # revealed: Unknown
```

Generic inference should also combine specializations found through the MRO of intersected concrete
subclasses, rather than only direct generic instances such as `Sequence[Sub1]` above:

```py
SourceT = TypeVar("SourceT", covariant=True)
ElementT = TypeVar("ElementT")

class Source(Generic[SourceT]):
    def get(self) -> SourceT:
        raise NotImplementedError

class A: ...
class B: ...
class ASource(Source[A]): ...
class BSource(Source[B]): ...
class IntSource(Source[int]): ...
class StrSource(Source[str]): ...

def element(x: Source[ElementT]) -> ElementT:
    return x.get()

def f(x: ASource) -> None:
    if isinstance(x, BSource):
        reveal_type(x)  # revealed: ASource & BSource
        reveal_type(element(x))  # revealed: A & B

def f(x: IntSource) -> None:
    if isinstance(x, StrSource):
        reveal_type(x)  # revealed: IntSource & StrSource
        reveal_type(element(x))  # revealed: Never
```

A constructor's synthetic `cls` argument can contain an inferable class type variable even when its
declared `cls` parameter is specialized to a concrete type:

```py
class ConcreteElement: ...

class FixedReceiverConstructor(Generic[ElementT]):
    item: ElementT

    def __new__(
        cls: "type[FixedReceiverConstructor[ConcreteElement]]",
        value: Source[ElementT],
    ) -> "FixedReceiverConstructor[ElementT]":
        raise NotImplementedError

def _(value: Intersection[Source[A], Source[ConcreteElement]]) -> None:
    reveal_type(FixedReceiverConstructor(value))  # revealed: FixedReceiverConstructor[ConcreteElement]
```

Generic constructors still reconstruct their return type from merged type-variable assignments, so
an iterable intersection does not yet refine the constructed list's element type:

```py
def explicit(x: Intersection[Sequence[int], str]) -> None:
    # TODO: revealed: list[Never]
    reveal_type(list(x))  # revealed: list[int | str]

def narrowed(x: Sequence[int]) -> None:
    if isinstance(x, str):
        reveal_type(x)  # revealed: Sequence[int] & str
        # TODO: revealed: list[Never]
        reveal_type(list(x))  # revealed: list[int | str]
```

Intersecting covariant return types does not generally allow intersecting their type arguments. A
meet-preserving generic would satisfy `F[A & B] == F[A] & F[B]`; covariance only guarantees
`F[A & B] <: F[A] & F[B]`, not the reverse. Here, an object usable as both `F[A]` and `F[B]` can
call its callback with an `A` or a `B`, respectively. It cannot safely accept a callback that only
handles values that are both `A` and `B`, as `F[A & B]` would allow. Thus `F` is not
meet-preserving, and inferring `F[A & B]` from `F[A] & F[B]` would be unsound:

```py
from typing import Callable

FSourceT = TypeVar("FSourceT", covariant=True)
FElementT = TypeVar("FElementT")

class F(Generic[FSourceT]):
    def use(self, callback: Callable[[FSourceT], int]) -> int:
        raise NotImplementedError

def return_f(x: F[FElementT]) -> F[FElementT]:
    return x

def takes_f_intersection(x: F[Intersection[A, B]]) -> None: ...
def _(x: Intersection[F[A], F[B]]) -> None:
    # `F[A] & F[B]` is not assignable to `F[A & B]`.
    # error: [invalid-argument-type]
    takes_f_intersection(x)
    # This cannot safely be `F[A & B]`.
    reveal_type(return_f(x))  # revealed: F[A] & F[B]
```

A gradual return component unrelated to inference does not invalidate either static path:

```py
from typing import Any

def element_with_any(x: Source[ElementT]) -> tuple[ElementT, Any]:
    return x.get(), None

def _(x: Intersection[Source[A], Source[B]]) -> None:
    reveal_type(element_with_any(x))  # revealed: tuple[A, Any] & tuple[B, Any]
```

Constraints from every argument are solved together before each valid call specialization is
instantiated:

```py
from typing import ParamSpec
from typing_extensions import TypeVarTuple, Unpack

class D: ...

def correlated(x: Source[ElementT], y: Source[ElementT]) -> ElementT:
    raise NotImplementedError

def _(
    x: Intersection[Source[A], Source[B]],
    y: Intersection[Source[B], Source[D]],
) -> None:
    reveal_type(correlated(x, y))  # revealed: (B & A) | (B & D)

def invariant_correlated(x: Box[ElementT], y: Box[ElementT]) -> ElementT:
    raise NotImplementedError

P = ParamSpec("P")
Ts = TypeVarTuple("Ts")

def invariant_paramspec(x: Box[ElementT], y: Box[ElementT], callback: Callable[P, None]) -> ElementT:
    raise NotImplementedError

def invariant_typevartuple(x: Box[ElementT], y: Box[ElementT], values: tuple[Unpack[Ts]]) -> ElementT:
    raise NotImplementedError

def _(
    x: Intersection[Box[A], Marker],
    y: Intersection[Box[B], D],
) -> None:
    # error: [invalid-argument-type] "Argument to function `invariant_correlated` is incorrect: Expected `Box[A | B]`, found `Box[A] & Marker`"
    # error: [invalid-argument-type] "Argument to function `invariant_correlated` is incorrect: Expected `Box[A | B]`, found `Box[B] & D`"
    reveal_type(invariant_correlated(x, y))  # revealed: A | B
    # error: [invalid-argument-type] "Argument to function `invariant_paramspec` is incorrect: Expected `Box[A | B]`, found `Box[A] & Marker`"
    # error: [invalid-argument-type] "Argument to function `invariant_paramspec` is incorrect: Expected `Box[A | B]`, found `Box[B] & D`"
    reveal_type(invariant_paramspec(x, y, lambda value: None))  # revealed: A | B
    # error: [invalid-argument-type] "Argument to function `invariant_typevartuple` is incorrect: Expected `Box[A | B]`, found `Box[A] & Marker`"
    # error: [invalid-argument-type] "Argument to function `invariant_typevartuple` is incorrect: Expected `Box[A | B]`, found `Box[B] & D`"
    reveal_type(invariant_typevartuple(x, y, (1, "x")))  # revealed: A | B
```

Callable constraints can add their own alternatives to the call-wide constraint set. Each complete
static specialization still validates the whole call and contributes its instantiated return:

```py
from typing import overload

def with_callback(x: Source[ElementT], callback: Callable[[ElementT], None]) -> ElementT:
    raise NotImplementedError

@overload
def accepts(value: A) -> None: ...
@overload
def accepts(value: B) -> None: ...
def accepts(value: A | B) -> None: ...
def _(x: Intersection[Source[A], Source[B]]) -> None:
    reveal_type(with_callback(x, accepts))  # revealed: A & B
```

Homogeneous unpacked tuple annotations on starred parameters can be validated for each
specialization, just like `*args: ElementT`. Both direct and unpacked arguments contribute to the
inferred return type:

```py
def with_starred(x: Source[ElementT], *args: Unpack[tuple[ElementT, ...]]) -> ElementT:
    raise NotImplementedError

def _(x: Intersection[Source[A], Source[B]], value: D, values: tuple[D, ...]) -> None:
    reveal_type(with_starred(x, value))  # revealed: (A & B) | D
    reveal_type(with_starred(x, *values))  # revealed: (A & B) | D
```

Intersection inference respects both generic variance and the polarity of nested comparisons:

```py
SinkT = TypeVar("SinkT", contravariant=True)

class Sink(Generic[SinkT]):
    def put(self, value: SinkT) -> None: ...

class ASink(Sink[A]): ...
class BSink(Sink[B]): ...

def sink_type(x: Sink[ElementT]) -> ElementT:
    raise NotImplementedError

def _(x: ASink) -> None:
    if isinstance(x, BSink):
        reveal_type(sink_type(x))  # revealed: A & B

class C(A, B): ...

def choose(x: ElementT, sink: Sink[Source[ElementT]]) -> ElementT:
    return x

def _(
    x: C,
    sink: Sink[Intersection[Source[A], Source[B]]],
) -> None:
    reveal_type(choose(x, sink))  # revealed: C
```

Generic inference keeps the known types contributed by gradual intersections, but does not yet
preserve their gradual components or intersect the independently inferred return types. Direct
member access shows the more precise types:

```py
def _(x) -> None:
    assert isinstance(x, ASource)
    reveal_type(x.get())  # revealed: Unknown & A
    # TODO: revealed: Unknown & A
    reveal_type(element(x))  # revealed: A
    assert isinstance(x, BSource)
    reveal_type(x.get())  # revealed: Unknown & A & B
    # TODO: revealed: Unknown & A & B
    reveal_type(element(x))  # revealed: A | B

def _(x: Any) -> None:
    assert isinstance(x, ASource)
    reveal_type(x.get())  # revealed: Any & A
    # TODO: revealed: Any & A
    reveal_type(element(x))  # revealed: A
    assert isinstance(x, BSource)
    reveal_type(x.get())  # revealed: Any & A & B
    # TODO: revealed: Any & A & B
    reveal_type(element(x))  # revealed: A | B
```

A narrowed gradual argument still contributes its known element type when another argument also
constrains `ElementT`. Ignoring the source's element type would infer an integer-only result and
hide an invalid attribute access:

```py
def element_with_value(value: ElementT, source: Source[ElementT]) -> ElementT:
    return source.get()

def _(unknown, any_: Any) -> None:
    assert isinstance(unknown, StrSource)
    assert isinstance(any_, StrSource)
    unknown_result = element_with_value(1, unknown)
    any_result = element_with_value(1, any_)
    reveal_type(unknown_result)  # revealed: Literal[1] | str
    reveal_type(any_result)  # revealed: Literal[1] | str
    # error: [unresolved-attribute]
    unknown_result.bit_length()
    # error: [unresolved-attribute]
    any_result.bit_length()
```

Unrelated gradual arguments do not affect `ElementT` and should not prevent static paths from
refining the return type:

```py
def element_with_other(x: Source[ElementT], other: object) -> ElementT:
    return x.get()

def element_with_dynamic_formal(x: Source[ElementT], other: Any) -> ElementT:
    return x.get()

def _(x: Intersection[Source[A], Source[B]], other) -> None:
    reveal_type(element_with_other(x, other))  # revealed: A & B
    reveal_type(element_with_dynamic_formal(x, A()))  # revealed: A & B
```

A gradual argument's known element type also contributes when another argument is an intersection.
The result includes `D`, but still unions the first argument's `A` and `B` contributions instead of
intersecting them:

```py
class DSource(Source[D]): ...

def _(x: Intersection[Source[A], Source[B]], unknown, any_: Any) -> None:
    assert isinstance(unknown, DSource)
    assert isinstance(any_, DSource)
    reveal_type(unknown.get())  # revealed: Unknown & D
    reveal_type(any_.get())  # revealed: Any & D
    # TODO: revealed: (A & B) | (Unknown & D)
    reveal_type(correlated(x, unknown))  # revealed: A | D | B
    # TODO: revealed: (A & B) | (Any & D)
    reveal_type(correlated(x, any_))  # revealed: A | D | B
```

A gradual intersection member can satisfy a declared bound or constraint even when its static
sibling cannot:

```py
BoundedT = TypeVar("BoundedT", bound=int)
ConstrainedT = TypeVar("ConstrainedT", int, bytes)

def bounded_element(source: Source[BoundedT]) -> BoundedT:
    return source.get()

def constrained_element(source: Source[ConstrainedT]) -> ConstrainedT:
    return source.get()

def _(unknown, any_: Any) -> None:
    assert isinstance(unknown, StrSource)
    assert isinstance(any_, StrSource)
    reveal_type(bounded_element(unknown))  # revealed: Unknown
    reveal_type(constrained_element(unknown))  # revealed: Unknown
    reveal_type(bounded_element(any_))  # revealed: Unknown
    reveal_type(constrained_element(any_))  # revealed: Unknown
```

When both members specialize `Source`, their intersection simplifies to `Source[str & Any]`, and
inference preserves that element type:

```py
def _(nested: Intersection[Source[Any], Source[str]]) -> None:
    reveal_type(nested)  # revealed: Source[str & Any]
    reveal_type(bounded_element(nested))  # revealed: str & Any
    reveal_type(constrained_element(nested))  # revealed: str & Any
```

Calls also remain valid when concrete subclasses inherit gradual specializations of `Source`. These
sibling classes remain separate intersection members:

```py
from ty_extensions._internal import Unknown

class AnySource(Source[Any]): ...
class UnknownSource(Source[Unknown]): ...

def _(
    any_: Intersection[AnySource, StrSource],
    unknown: Intersection[UnknownSource, StrSource],
) -> None:
    reveal_type(bounded_element(any_))  # revealed: Any
    reveal_type(constrained_element(any_))  # revealed: Any
    reveal_type(bounded_element(unknown))  # revealed: Unknown
    reveal_type(constrained_element(unknown))  # revealed: Unknown
```

An untyped value narrowed to `list` remains gradual. `enumerate` should accept it and preserve the
unknown element type:

```py
def _(x):
    assert isinstance(x, list)
    for _, item in enumerate(x):
        reveal_type(item)  # revealed: Unknown
```

## Intersection arguments do not hide argument errors

Every specialization of a generic call must accept all of its arguments, including parameters
without type variables. The intersection below provides two choices for `T`, but neither choice
makes a string a valid `int` argument. The revealed return type comes from error recovery, not from
a valid call specialization.

```py
from typing import Generic, TypeVar
from ty_extensions import Intersection

SourceT = TypeVar("SourceT", covariant=True)
T = TypeVar("T")
U = TypeVar("U")

class Source(Generic[SourceT]):
    def get(self) -> SourceT:
        raise NotImplementedError

class A: ...
class B: ...

def with_fixed(source: Source[T], other: int) -> T:
    return source.get()

def _(source: Intersection[Source[A], Source[B]]) -> None:
    # error: [invalid-argument-type]
    reveal_type(with_fixed(source, "bad"))  # revealed: A | B
```

An invalid union member still makes an argument incompatible, even when its other member provides a
type-variable assignment. Neither choice for `T` makes `None` a valid `list[U]` argument:

```py
def with_list(source: Source[T], values: list[U]) -> tuple[T, U]:
    return source.get(), values[0]

def _(source: Intersection[Source[A], Source[B]], values: list[int] | None) -> None:
    # error: [invalid-argument-type]
    reveal_type(with_list(source, values))  # revealed: tuple[A | B, int]
```

The same applies to a tuple with an incompatible non-generic element. Inferring `U = int` from the
first element does not make the second element's `str` type compatible with `int`:

```py
def with_tuple(source: Source[T], value: tuple[U, int]) -> tuple[T, U]:
    return source.get(), value[0]

def _(source: Intersection[Source[A], Source[B]], value: tuple[int, str]) -> None:
    # error: [invalid-argument-type]
    reveal_type(with_tuple(source, value))  # revealed: tuple[A | B, int]
```

## Outer type variables in intersection arguments

When one generic function calls another, an argument can contain a type variable belonging to the
caller. The called function cannot choose a narrower meaning for that variable just to make the call
succeed. The call must work for every type permitted by the caller's signature, not just for a type
that happens to satisfy the called function's bound or constraints.

A caller that accepts a source of any element type therefore cannot pass it to a function that
requires a source of strings. Likewise, a source whose elements may be integers or bytes cannot be
passed to a function that requires integer or string elements: the bytes case is still possible.
Knowing that the source also has an unrelated type does not restrict its element types, so adding
that type to an intersection does not make either call valid.

```py
from typing import Generic, TypeVar
from ty_extensions import Intersection

SourceT = TypeVar("SourceT", covariant=True)
BoundedT = TypeVar("BoundedT", bound=str)
ConstrainedT = TypeVar("ConstrainedT", int, str)
OuterT = TypeVar("OuterT")
OuterConstrainedT = TypeVar("OuterConstrainedT", int, bytes)

class Source(Generic[SourceT]):
    def get(self) -> SourceT:
        raise NotImplementedError

class Marker: ...

def first_bounded(value: Source[BoundedT]) -> BoundedT:
    return value.get()

def first_constrained(value: Source[ConstrainedT]) -> ConstrainedT:
    return value.get()

def plain_bounded(value: Source[OuterT]) -> None:
    # error: [invalid-argument-type]
    reveal_type(first_bounded(value))  # revealed: Unknown

def intersected_bounded(value: Intersection[Source[OuterT], Marker]) -> None:
    # error: [invalid-argument-type]
    reveal_type(first_bounded(value))  # revealed: Unknown

def plain_constrained(value: Source[OuterConstrainedT]) -> None:
    # error: [invalid-argument-type]
    reveal_type(first_constrained(value))  # revealed: Unknown

def intersected_constrained(value: Intersection[Source[OuterConstrainedT], Marker]) -> None:
    # error: [invalid-argument-type]
    reveal_type(first_constrained(value))  # revealed: Unknown
```

Compatible outer variables retain their identity. The bound allows every type represented by
`OuterStrT`, and every constraint of `OuterCompatibleT` is also a constraint of `ConstrainedT`:

```py
OuterStrT = TypeVar("OuterStrT", bound=str)
OuterCompatibleT = TypeVar("OuterCompatibleT", int, str)

def compatible_bounded(value: Intersection[Source[OuterStrT], Marker]) -> None:
    reveal_type(first_bounded(value))  # revealed: OuterStrT@compatible_bounded

def compatible_constrained(value: Intersection[Source[OuterCompatibleT], Marker]) -> None:
    reveal_type(first_constrained(value))  # revealed: OuterCompatibleT@compatible_constrained
```

A separate valid intersection element can still satisfy the parameter. The outer variable does not
meet the declaration, but `Source[str]` supplies the valid specialization:

```py
def valid_bounded_element(value: Intersection[Source[OuterT], Source[str]]) -> None:
    reveal_type(first_bounded(value))  # revealed: str

def valid_constrained_element(value: Intersection[Source[OuterConstrainedT], Source[str]]) -> None:
    reveal_type(first_constrained(value))  # revealed: str
```

Lazy type aliases do not hide an outer variable from declaration checks. An alias of a variable with
a compatible bound still preserves that variable's identity:

```py
from typing_extensions import TypeAliasType

AliasT = TypeVar("AliasT")
Alias = TypeAliasType("Alias", AliasT, type_params=(AliasT,))

def aliased_bounded(value: Intersection[Source[Alias[OuterT]], Marker]) -> None:
    # error: [invalid-argument-type]
    reveal_type(first_bounded(value))  # revealed: Unknown

def aliased_compatible(value: Intersection[Source[Alias[OuterStrT]], Marker]) -> None:
    reveal_type(first_bounded(value))  # revealed: OuterStrT@aliased_compatible
```
