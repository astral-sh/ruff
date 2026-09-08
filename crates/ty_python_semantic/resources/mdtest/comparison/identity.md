# Identity comparisons

## Basic comparisons

```py
from typing_extensions import TypeAliasType

reveal_type(False is False)  # revealed: Literal[True]
reveal_type(False is True)  # revealed: Literal[False]
reveal_type(1 is True)  # revealed: Literal[False]
reveal_type(... is ...)  # revealed: Literal[True]
reveal_type(NotImplemented is NotImplemented)  # revealed: Literal[True]

# two occurences of the same literal `1` do not necessarily share the
# same memory address, as `1` is not a singleton (but they also *might*!)
reveal_type(1 is 1)  # revealed: bool

# but two different integer literals definitely don't share the same memory address
reveal_type(1 is 2)  # revealed: Literal[False]

class A: ...

def _(a1: A, a2: A, o: object):
    n1 = None
    n2 = None

    reveal_type(a1 is a1)  # revealed: bool
    reveal_type(a1 is a2)  # revealed: bool

    reveal_type(n1 is n1)  # revealed: Literal[True]
    reveal_type(n1 is n2)  # revealed: Literal[True]

    reveal_type(a1 is n1)  # revealed: Literal[False]
    reveal_type(n1 is a1)  # revealed: Literal[False]

    reveal_type(a1 is o)  # revealed: bool
    reveal_type(n1 is o)  # revealed: bool

    reveal_type(a1 is not a1)  # revealed: bool
    reveal_type(a1 is not a2)  # revealed: bool

    reveal_type(n1 is not n1)  # revealed: Literal[False]
    reveal_type(n1 is not n2)  # revealed: Literal[False]

    reveal_type(a1 is not n1)  # revealed: Literal[True]
    reveal_type(n1 is not a1)  # revealed: Literal[True]

    reveal_type(a1 is not o)  # revealed: bool
    reveal_type(n1 is not o)  # revealed: bool

def _(a1: TypeAliasType, a2: TypeAliasType):
    reveal_type(a1 is a2)  # revealed: bool
    reveal_type(a1 is not a2)  # revealed: bool

reveal_type(list[int] is list[int])  # revealed: bool
reveal_type(list[int] is not list[int])  # revealed: bool
```

## Function identity after passing through a generic identity function

Passing a function through a generic identity function preserves the function object, so
`identity(f) is f` is correctly inferred as `Literal[True]` in the examples below:

```py
from typing import TypeVar

F = TypeVar("F")

def identity(value: F) -> F:
    return value

def f():
    pass

reveal_type(identity(f) is f)  # revealed: Literal[True]
reveal_type(f is identity(f))  # revealed: Literal[True]

reveal_type(identity(f) is not f)  # revealed: Literal[False]
reveal_type(f is not identity(f))  # revealed: Literal[False]

def g():
    pass

reveal_type(identity(f) is g)  # revealed: Literal[False]
reveal_type(g is identity(f))  # revealed: Literal[False]

reveal_type(identity(f) is not g)  # revealed: Literal[True]
reveal_type(g is not identity(f))  # revealed: Literal[True]
```

## Identity comparisons between function specializations

Different specializations of the same unbound method have disjoint static types but refer to the
same function object at runtime:

```toml
[environment]
python-version = "3.12"
```

```py
class C[T]:
    def method(self, value: T) -> T:
        return value

int_method = C[int].method
str_method = C[str].method
reveal_type(int_method is str_method)  # revealed: Literal[True]
reveal_type(int_method is not str_method)  # revealed: Literal[False]
```

## Bound method identity

Accessing a method creates a bound method object, so even two references with the same bound method
type need not identify the same object. A generic identity function and `functools.partial` both
retain the bound method object passed to them. Its signature may be specialized by those calls, but
that does not make an identity comparison with the saved method always false.

```py
from functools import partial
from typing import TypeVar, final

T = TypeVar("T")

def identity(value: T) -> T:
    return value

class C:
    @final
    def method(self, value: int) -> int:
        return value

saved_method = C().method
reveal_type(saved_method is saved_method)  # revealed: bool
reveal_type(identity(saved_method) is saved_method)  # revealed: bool
reveal_type(saved_method is identity(saved_method))  # revealed: bool
reveal_type(identity(saved_method) is not saved_method)  # revealed: bool
reveal_type(partial(saved_method).func is saved_method)  # revealed: bool

reveal_type(identity(saved_method)(1))  # revealed: int

class D:
    @final
    def method(self, value: int) -> int:
        return value

reveal_type(saved_method is D().method)  # revealed: Literal[False]
```

`NewType` constructors also leave the receiver object unchanged. Bound method types with different
`NewType` tags on their receivers are statically disjoint, but can describe the same method object.

```py
from typing import NewType
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_disjoint_from

Left = NewType("Left", C)
Right = NewType("Right", C)

static_assert(is_disjoint_from(TypeOf[Left(C()).method], TypeOf[Right(C()).method]))

def compare(left: TypeOf[Left(C()).method], right: TypeOf[Right(C()).method]) -> None:
    reveal_type(left is right)  # revealed: bool
```

## Identity of properties, saved method wrappers, and partials

Specializing a generic class changes the static signatures of its property's accessor and unbound
method. The property, a `functools.partial` of the method, and the wrappers saved on the class are
each created once. Their views through `C[int]` and `C[str]` can therefore identify the same object.

```toml
[environment]
python-version = "3.12"
```

```py
from functools import partial

class C[T]:
    @property
    def prop(self) -> T:
        raise NotImplementedError

    def method(self, value: T) -> T:
        return value

    callback = partial(method)
    callback_call = callback.__call__
    method_getter = method.__get__
    method_caller = method.__call__

reveal_type(C[int].prop is C[str].prop)  # revealed: bool
reveal_type(C[int].prop is not C[str].prop)  # revealed: bool
reveal_type(C[int].callback is C[str].callback)  # revealed: bool
reveal_type(C[int].callback_call is C[str].callback_call)  # revealed: bool
reveal_type(C[int].method_getter is C[str].method_getter)  # revealed: bool
reveal_type(C[int].method_caller is C[str].method_caller)  # revealed: bool
```

The callable signatures remain specialized for calls. Different property definitions, partials
wrapping different functions, and the two saved wrappers of `method` still describe distinct
objects.

```py
class D:
    @property
    def prop(self) -> int:
        return 0

def other(value: int) -> int:
    return value

def another(value: int) -> int:
    return value

other_callback = partial(other)
another_callback = partial(another)

reveal_type(C[int].callback(C[int](), 1))  # revealed: int
reveal_type(C[str].callback(C[str](), "value"))  # revealed: str
reveal_type(C[int].prop is D.prop)  # revealed: Literal[False]
reveal_type(other_callback is another_callback)  # revealed: Literal[False]
reveal_type(C[int].method_getter is C[int].method_caller)  # revealed: Literal[False]
```

## Identity comparisons with NewTypes

Two variables cannot share the same memory address if they have disjoint nominal-instance backing
types:

```py
def f(x: str, y: int):
    reveal_type(x is y)  # revealed: Literal[False]
    reveal_type(x is not y)  # revealed: Literal[True]
```

Distinct `NewType` tags are mutually exclusive, so their types are disjoint. Their constructors
still return their arguments unchanged: `B(True)` and `C(True)` have different tags but share the
same memory address, so an identity comparison can succeed.

```py
from typing import NewType, Literal
from ty_extensions._internal import is_disjoint_from

B = NewType("B", bool)
C = NewType("C", bool)

reveal_type(is_disjoint_from(B, C))  # revealed: ConstraintSet[Literal[True]]
reveal_type(is_disjoint_from(B, Literal[True]))  # revealed: ConstraintSet[Literal[False]]

def f(x: B, y: C):
    reveal_type(x is y)  # revealed: bool
    reveal_type(x is not y)  # revealed: bool
    reveal_type(x is True)  # revealed: bool
    reveal_type(x is False)  # revealed: bool
    reveal_type(x is not True)  # revealed: bool
    reveal_type(x is not False)  # revealed: bool
```

Nonetheless, if the NewType's nominal backing type is disjoint from another type, `Literal` boolean
types can still be inferred as a result:

```py
from typing import NewType, Literal

N = NewType("N", str)
O = NewType("O", int)

def f(x: N, y: int, z: O):
    reveal_type(x is y)  # revealed: Literal[False]
    reveal_type(x is not y)  # revealed: Literal[True]
    reveal_type(x is z)  # revealed: Literal[False]
    reveal_type(x is not z)  # revealed: Literal[True]
```

## Identity comparisons with type guard results

`TypeIs` and `TypeGuard` functions return booleans, so comparing their results with `True` or
`False` can succeed or fail.

```py
from typing_extensions import TypeGuard, TypeIs

def is_int(x: object) -> TypeIs[int]:
    return isinstance(x, int)

def is_int_guard(x: object) -> TypeGuard[int]:
    return isinstance(x, int)

def f(x: object):
    reveal_type(is_int(x) is True)  # revealed: bool
    reveal_type(is_int(x) is False)  # revealed: bool
    reveal_type(is_int_guard(x) is True)  # revealed: bool
    reveal_type(is_int_guard(x) is False)  # revealed: bool
```

## Identity comparisons see through type aliases

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal

type SoTrue = Literal[True]
type SoFalse = Literal[False]

def f(x: SoTrue, y: SoFalse):
    reveal_type(x is True)  # revealed: Literal[True]
    reveal_type(x is False)  # revealed: Literal[False]
    reveal_type(x is y)  # revealed: Literal[False]
    reveal_type(x is not y)  # revealed: Literal[True]
```

## Repeated identity comparisons after narrowing `Unknown`

Once `value is None` has succeeded, the value can only be the `None` singleton even when its
original type is `Unknown`.

```py
from ty_extensions._internal import Unknown

def f(value: Unknown) -> None:
    if value is None:
        reveal_type(value)  # revealed: Unknown & None
        reveal_type(value is not None)  # revealed: Literal[False]
```

## Identity comparisons for the same constrained `TypeVar`

All occurrences of the same constrained `TypeVar` use the same constraint. Here, each constraint
contains only one object, so two values with that `TypeVar` must be identical. This remains true
when one occurrence appears through a type alias.

```toml
[environment]
python-version = "3.12"
```

```py
from types import EllipsisType
from typing import TypeVar

T = TypeVar("T", None, EllipsisType)

def f(left: T, right: T) -> None:
    reveal_type(left is right)  # revealed: Literal[True]

type Alias[X] = X

def aliased(left: Alias[T], right: T) -> None:
    reveal_type(left is right)  # revealed: Literal[True]
```
