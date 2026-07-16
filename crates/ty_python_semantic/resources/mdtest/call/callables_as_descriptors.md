# Callables as descriptors?

```toml
[environment]
python-version = "3.14"
```

## Introduction

Some common callable objects (all functions, including lambdas) are also bound-method descriptors.
That is, they have a `__get__` method which returns a bound-method object that binds the receiver
instance to the first argument. The bound-method object therefore has a different signature, lacking
the first argument:

```py
from ty_extensions._internal import RegularCallableTypeOf
from typing import Any, Callable

class C1:
    def method(self: C1, x: int) -> str:
        return str(x)

def _(
    accessed_on_class: RegularCallableTypeOf[C1.method],
    accessed_on_instance: RegularCallableTypeOf[C1().method],
):
    reveal_type(accessed_on_class)  # revealed: (self: C1, x: int) -> str
    reveal_type(accessed_on_instance)  # revealed:        (x: int) -> str
```

Other callable objects (`staticmethod` objects, instances of classes with a `__call__` method but no
dedicated `__get__` method) are *not* bound-method descriptors. If accessed as class attributes via
an instance, they are simply themselves:

```py
class NonDescriptorCallable2:
    def __call__(self, c2: C2, x: int) -> str:
        return str(x)

class C2:
    non_descriptor_callable: NonDescriptorCallable2 = NonDescriptorCallable2()

def _(
    accessed_on_class: RegularCallableTypeOf[C2.non_descriptor_callable],
    accessed_on_instance: RegularCallableTypeOf[C2().non_descriptor_callable],
):
    reveal_type(accessed_on_class)  # revealed:    (c2: C2, x: int) -> str
    reveal_type(accessed_on_instance)  # revealed: (c2: C2, x: int) -> str
```

Both kinds of objects can inhabit the same `Callable` type:

```py
class NonDescriptorCallable3:
    def __call__(self, c3: C3, x: int) -> str:
        return str(x)

class C3:
    def method(self: C3, x: int) -> str:
        return str(x)
    non_descriptor_callable: NonDescriptorCallable3 = NonDescriptorCallable3()

    callable_m: Callable[[C3, int], str] = method
    callable_n: Callable[[C3, int], str] = non_descriptor_callable
```

However, when they are accessed on instances of `C3`, they have different signatures:

```py
def _(
    method_accessed_on_instance: RegularCallableTypeOf[C3().method],
    callable_accessed_on_instance: RegularCallableTypeOf[C3().non_descriptor_callable],
):
    reveal_type(method_accessed_on_instance)  # revealed:           (x: int) -> str
    reveal_type(callable_accessed_on_instance)  # revealed: (c3: C3, x: int) -> str
```

This leaves the question how the `callable_m` and `callable_n` attributes should be treated when
accessed on instances of `C3`. If we treat `Callable` as being equivalent to a protocol that defines
a `__call__` method (and no `__get__` method), then they should show no bound-method behavior. This
is what we currently do:

```py
reveal_type(C3().callable_m)  # revealed: (C3, int, /) -> str
reveal_type(C3().callable_n)  # revealed: (C3, int, /) -> str
```

However, this leads to unsoundness: `C3().callable_m` is actually `C3.method` which *is* a
bound-method descriptor. We currently allow the following call, which will fail at runtime:

```py
C3().callable_m(C3(), 1)  # runtime error! ("takes 2 positional arguments but 3 were given")
```

If we were to treat `Callable`s as bound-method descriptors, then the signatures of `callable_m` and
`callable_n` when accessed on instances would bind the `self` argument:

- `C3().callable_m`: `(x: int) -> str`
- `C3().callable_n`: `(x: int) -> str`

This would be equally unsound, because now we would allow a call to `C3().callable_n(1)` which would
also fail at runtime.

There is no perfect solution here, but for compatibility with other type checkers we treat inferred
class-body callables and `ClassVar`-annotated callables with a positional receiver as bound-method
descriptors. Wholly gradual signatures have no known receiver to bind during ordinary attribute
access, and explicit non-`ClassVar` callable annotations remain regular. Concrete callable dunders
are treated as methods when invoked implicitly; callable-bounded TypeVars and ParamSpecs remain
regular. Gradual callable dunders retain function-like behavior for class-level inspection.

## Callable descriptor lookup modes

For inferred and `ClassVar` callables, promotion to a bound-method descriptor only affects instance
reads. Class reads and writes retain the declared callable signature. This also applies when the
callable is hidden behind a PEP 695 type alias or has a gradual `Concatenate` tail:

```py
from collections.abc import Callable
from typing import Any, ClassVar, Concatenate, cast

def method(value: object, argument: str) -> int:
    return len(argument)

def gradual_method(value: object, *args: Any, **kwargs: Any) -> int:
    return 1

type Method = Callable[[object, str], int]
type GradualMethod = Callable[Concatenate[object, ...], int]

class C:
    inferred = cast(Method, method)
    inferred_gradual = cast(GradualMethod, gradual_method)
    classvar: ClassVar[Method] = method
    classvar_gradual: ClassVar[GradualMethod] = gradual_method
    explicit: Method = method

reveal_type(C.inferred)  # revealed: (object, str, /) -> int
reveal_type(C().inferred)  # revealed: (str, /) -> int
reveal_type(C.classvar)  # revealed: (object, str, /) -> int
reveal_type(C().classvar)  # revealed: (str, /) -> int
reveal_type(C().explicit)  # revealed: (object, str, /) -> int

C.inferred = method
C.classvar = method

reveal_type(C().inferred("value"))  # revealed: int
reveal_type(C().classvar("value"))  # revealed: int
reveal_type(C().inferred_gradual())  # revealed: int
reveal_type(C().classvar_gradual())  # revealed: int
```

## Use case: Decorating a method with a `Callable`-typed decorator

A commonly used pattern in the ecosystem is to use a `Callable`-typed decorator on a method with the
intention that it shouldn't influence the method's descriptor behavior. For example, we treat
`method_decorated` below as a bound method, even though its type is `Callable[[C1, int], str]`:

```py
from typing import Any, Callable

def memoize[**P, R](f: Callable[P, R]) -> Callable[P, R]:
    raise NotImplementedError

class C1:
    def method(self, x: int) -> str:
        return str(x)

    @memoize
    def method_decorated(self, x: int) -> str:
        return str(x)

C1().method(1)

C1().method_decorated(1)
```

This also works with an argumentless `Callable` annotation:

```py
def memoize2(f: Callable[..., Any]) -> Callable[..., Any]:
    raise NotImplementedError

class C2:
    @memoize2
    def method_decorated(self, x: int) -> str:
        return str(x)

C2().method_decorated(1)
```

And if the callable-typed decorator leaves some generic parameters unconstrained, we should keep
those parameters unspecialized rather than collapsing them to `Never`:

```py
def passthrough[T, R](f: Callable[[T], R]) -> Callable[[T], R]:
    raise NotImplementedError

@passthrough
def f(x):
    return x

reveal_type(f)  # revealed: (Unknown, /) -> Unknown
reveal_type(f(1))  # revealed: Unknown
```

And with unions of `Callable` types:

```py
from typing import Callable

def expand(f: Callable[[C3, int], int]) -> Callable[[C3, int], int] | Callable[[C3, int], str]:
    raise NotImplementedError

class C3:
    @expand
    def method_decorated(self, x: int) -> int:
        return x

reveal_type(C3().method_decorated(1))  # revealed: int | str
```

Transparent decorators are also treated consistently when spelled as an equivalent assignment:

```py
class C4:
    def method(self, x: int) -> str:
        return str(x)
    method_decorated = memoize(method)

C4().method_decorated(1)
```

For non-transparent decorators, avoid resolving the decorated function's signature before the
decorator itself has been rejected. Doing so can introduce a cycle when the signature refers back to
the decorated name:

```py
decorated = lambda: decorated
try:
    pass
except* Exception:
    pass

unknown_decorator: Any

@unknown_decorator  # error: [unresolved-reference]
def decorated(argument: lambda: decorated, /):  # error: [invalid-type-form]
    pass
```

This is necessarily unsound: a function call can return a `Callable` unrelated to the argument. The
following program is valid at runtime, but we reject it because the inferred class-body callable is
treated as a descriptor. This heuristic does not check whether the first parameter can accept the
instance:

```py
class SquareCalculator:
    def __init__(self, post_process: Callable[[float], int]):
        self.post_process = post_process

    def __call__(self, x: float) -> int:
        return self.post_process(x * x)

def square_then(c: Callable[[float], int]) -> Callable[[float], int]:
    return SquareCalculator(c)

class Calculator:
    square_then_round = square_then(round)

# error: [too-many-positional-arguments]
Calculator().square_then_round(3.14)
```

## Use case: Wrappers with explicit receivers

`trio` defines multiple functions that takes in a callable with `Concatenate`-prepended receiver
types, and returns a wrapper function with a different receiver type. They should still preserve
descriptor behavior when the returned callable is assigned in the class body.

```py
from collections.abc import Callable, Iterable
from typing import Concatenate, ParamSpec, TypeVar

P = ParamSpec("P")
T = TypeVar("T")

class RawPath:
    def write_bytes(self, data: bytes) -> int:
        raise NotImplementedError

def _wrap_method(
    fn: Callable[Concatenate[RawPath, P], T],
) -> Callable[Concatenate["Path", P], T]:
    raise NotImplementedError

class Path:
    write_bytes = _wrap_method(RawPath.write_bytes)

def check(path: Path) -> None:
    reveal_type(path.write_bytes(b""))  # revealed: int
```

## Use case: Treating dunder methods as bound-method descriptors

In the following example, the `__lt__` dunder attribute is not declared. The attribute type is
inferred as `Callable[…]`, so we treat it as a bound-method descriptor:

```py
from collections.abc import Callable

def make_comparison_operator(name: str) -> Callable[[Matrix, Matrix], bool]:
    raise NotImplementedError

class Matrix:
    __lt__ = make_comparison_operator("lt")

Matrix() < Matrix()
```

An explicitly annotated non-`ClassVar` callable dunder without a positional first parameter remains
a regular callable:

```py
class Thunk:
    __value_thunk__: Callable[[], int]

    def replace(self, other: "Thunk") -> None:
        self.__value_thunk__ = other.__value_thunk__

reveal_type(Thunk().__value_thunk__)  # revealed: () -> int
```

In particular, ordinary attribute access retains the full signature of a concrete callable dunder:

```py
def descriptor_candidate(value: str) -> int:
    return len(value)

class DescriptorCandidate:
    __value__: Callable[[str], int] = descriptor_candidate

reveal_type(DescriptorCandidate().__value__)  # revealed: (str, /) -> int
```

A gradual callable annotation on a dunder also describes a method, so function attributes are
available on the class member:

```py
from typing import Any

class Method:
    __call__: Callable[..., Any]

Method.__call__.__code__
```

## `self`-binding behaviour of function-like `Callable`s

Binding the `self` parameter of a function-like `Callable` creates a new `Callable` that is also
function-like:

`main.py`:

```py
from typing import Callable

def my_lossy_decorator(fn: Callable[..., int]) -> Callable[..., int]:
    return fn

class MyClass:
    @my_lossy_decorator
    def method(self) -> int:
        return 42

reveal_type(MyClass().method)  # revealed: (...) -> int
reveal_type(MyClass().method.__name__)  # revealed: str
```

## classmethods passed through Callable-returning decorators

The behavior described above is also applied to classmethods. If a method is decorated with
`@classmethod`, and also with another decorator which returns a Callable type, we make the
assumption that the decorator returns a callable which still has the classmethod descriptor
behavior.

```py
from typing import Callable

def callable_identity[**P, R](func: Callable[P, R]) -> Callable[P, R]:
    return func

class C:
    @callable_identity
    @classmethod
    def f1(cls, x: int) -> str:
        return "a"

    @classmethod
    @callable_identity
    def f2(cls, x: int) -> str:
        return "a"

# error: [too-many-positional-arguments]
# error: [invalid-argument-type]
C.f1(C, 1)
C.f1(1)
C().f1(1)
C.f2(1)
C().f2(1)
```

## Types are not bound-method descriptors

```toml
[environment]
python-version = "3.14"
```

The callable type of a type object is not function-like. Once that type is extracted into a regular
callable signature, the `ClassVar` descriptor heuristic applies on instance reads.

```py
from typing import ClassVar
from ty_extensions._internal import RegularCallableTypeOf, TypeOf

class WithNew:
    def __new__(self, x: int) -> WithNew:
        return super().__new__(WithNew)

class WithInit:
    def __init__(self, x: int) -> None:
        pass

class C:
    with_new: ClassVar[TypeOf[WithNew]]
    with_init: ClassVar[TypeOf[WithInit]]
    extracted_new: ClassVar[RegularCallableTypeOf[WithNew]]
    extracted_init: ClassVar[RegularCallableTypeOf[WithInit]]

C.with_new(1)
C().with_new(1)
C.with_init(1)
C().with_init(1)
C.extracted_new(1)
# error: [too-many-positional-arguments]
C().extracted_new(1)
C.extracted_init(1)
# error: [too-many-positional-arguments]
C().extracted_init(1)
```

## Decorators returning PEP 695 type aliases

When a decorator's return type is a PEP 695 type alias that wraps a `Callable` type, the decorated
method should still behave as a bound-method descriptor. This also works correctly with `super()`.

```py
from typing import Any
from collections.abc import Callable

type Func = Callable[[Any], str]

def noop(func: Func) -> Func:
    return func

class Base:
    @noop
    def foo(self) -> str:
        return "base"

class Derived(Base):
    @noop
    def foo(self) -> str:
        return super().foo()

reveal_type(Base().foo)  # revealed: () -> str
reveal_type(Derived().foo)  # revealed: () -> str

# These calls should work without errors
Base().foo()
Derived().foo()
```

The same applies to methods accessed via `super()` directly:

```py
from typing import Any
from collections.abc import Callable

type MethodType = Callable[[Any, int], str]

def decorator(func: MethodType) -> MethodType:
    return func

class Parent:
    @decorator
    def method(self, x: int) -> str:
        return str(x)

class Child(Parent):
    @decorator
    def method(self, x: int) -> str:
        # super().method should be a bound method, not require `self`
        return super().method(x)

reveal_type(Parent().method)  # revealed: (int, /) -> str
reveal_type(Child().method)  # revealed: (int, /) -> str

Parent().method(1)
Child().method(1)
```
