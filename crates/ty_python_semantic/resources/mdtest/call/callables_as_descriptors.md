# Callables as descriptors?

<!-- blacken-docs:off -->

```toml
[environment]
python-version = "3.14"
```

## Introduction

Some common callable objects (functions, lambdas) are also bound-method descriptors. That is, they
have a `__get__` method which returns a bound-method object that binds the receiver instance to the
first argument (and thus the bound-method object has a different signature, lacking the first
argument):

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
    reveal_type(accessed_on_class)     # revealed: (self: C1, x: int) -> str
    reveal_type(accessed_on_instance)  # revealed:           (x: int) -> str
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
    reveal_type(accessed_on_class)     # revealed: (c2: C2, x: int) -> str
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
    reveal_type(method_accessed_on_instance)    # revealed:         (x: int) -> str
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
intention that it shouldn't influence the method's descriptor behavior. For example:

```py
from typing import Callable

# TODO: this could use a generic signature, but we don't support
# `ParamSpec` and solving of typevars inside `Callable` types.
def memoize(f: Callable[[C, int], str]) -> Callable[[C, int], str]:
    raise NotImplementedError

class C:
    def method(self, x: int) -> str:
        return str(x)

    @memoize
    def method_decorated(self, x: int) -> str:
        return str(x)

C().method(1)

C().method_decorated(1)
```
