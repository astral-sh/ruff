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
from ty_extensions import CallableTypeOf
from typing import Callable

class C1:
    def method(self: C1, x: int) -> str:
        return str(x)

def _(
    accessed_on_class: CallableTypeOf[C1.method],
    accessed_on_instance: CallableTypeOf[C1().method],
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
    accessed_on_class: CallableTypeOf[C2.non_descriptor_callable],
    accessed_on_instance: CallableTypeOf[C2().non_descriptor_callable],
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
    method_accessed_on_instance: CallableTypeOf[C3().method],
    callable_accessed_on_instance: CallableTypeOf[C3().non_descriptor_callable],
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

There is no perfect solution here, but we can use some heuristics to improve the situation for
certain use cases (at the cost of purity and simplicity).

## Use case: Decorating a method with a `Callable`-typed decorator

A commonly used pattern in the ecosystem is to use a `Callable`-typed decorator on a method with the
intention that it shouldn't influence the method's descriptor behavior. For example, we treat
`method_decorated` below as a bound method, even though its type is `Callable[[C1, int], str]`:

```py
from typing import Callable

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
def memoize2(f: Callable) -> Callable:
    raise NotImplementedError

class C2:
    @memoize2
    def method_decorated(self, x: int) -> str:
        return str(x)

C2().method_decorated(1)
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

Note that we currently only apply this heuristic when calling a function such as `memoize` via the
decorator syntax. This is inconsistent, because the above *should* be equivalent to the following,
but here we emit errors:

```py
def memoize3(f: Callable[[C4, int], str]) -> Callable[[C4, int], str]:
    raise NotImplementedError

class C4:
    def method(self, x: int) -> str:
        return str(x)
    method_decorated = memoize3(method)

# error: [missing-argument]
# error: [invalid-argument-type]
C4().method_decorated(1)
```

The reason for this is that the heuristic is problematic. We don't *know* that the `Callable` in the
return type of `memoize` is actually related to the method that we pass in. But when `memoize` is
applied as a decorator, it is reasonable to assume so.

In general, a function call might however return a `Callable` that is unrelated to the argument
passed in. And here, it seems more reasonable and safe to treat the `Callable` as a non-descriptor.
This allows correct programs like the following to pass type checking (that are currently rejected
by pyright and mypy with a heuristic that apparently applies in a wider range of situations):

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

reveal_type(Calculator().square_then_round(3.14))  # revealed: Unknown | int
```

## Use case: Treating dunder methods as bound-method descriptors

pytorch defines a `__pow__` dunder attribute on [`TensorBase`] in a similar way to the following
example. We generally treat dunder attributes as bound-method descriptors since they all take a
`self` argument. This allows us to type-check the following code correctly:

```py
from typing import Callable

def pow_impl(tensor: Tensor, exponent: int) -> Tensor:
    raise NotImplementedError

class Tensor:
    __pow__: Callable[[Tensor, int], Tensor] = pow_impl

Tensor() ** 2
```

The following example is also taken from a real world project. Here, the `__lt__` dunder attribute
is not declared. The attribute type is therefore inferred as `Unknown | Callable[…]`, but we still
treat it as a bound-method descriptor:

```py
def make_comparison_operator(name: str) -> Callable[[Matrix, Matrix], bool]:
    raise NotImplementedError

class Matrix:
    __lt__ = make_comparison_operator("lt")

Matrix() < Matrix()
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

The callable type of a type object is not function-like.

```py
from typing import ClassVar
from ty_extensions import CallableTypeOf

class WithNew:
    def __new__(self, x: int) -> WithNew:
        return super().__new__(WithNew)

class WithInit:
    def __init__(self, x: int) -> None:
        pass

class C:
    with_new: ClassVar[CallableTypeOf[WithNew]]
    with_init: ClassVar[CallableTypeOf[WithInit]]

C.with_new(1)
C().with_new(1)
C.with_init(1)
C().with_init(1)
```

[`tensorbase`]: https://github.com/pytorch/pytorch/blob/f3913ea641d871f04fa2b6588a77f63efeeb9f10/torch/_tensor.py#L1084-L1092
