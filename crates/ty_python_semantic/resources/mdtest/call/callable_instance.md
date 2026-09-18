# Callable instance

## Dunder call

```py
class Multiplier:
    def __init__(self, factor: int):
        self.factor = factor

    def __call__(self, number: int) -> int:
        return number * self.factor

a = Multiplier(2)(3)
reveal_type(a)  # revealed: int

class Unit: ...

b = Unit()(3.0)  # error: "Object of type `Unit` is not callable"
reveal_type(b)  # revealed: Unknown
```

## Possibly missing `__call__` method

```py
def _(flag: bool):
    class PossiblyNotCallable:
        if flag:
            def __call__(self) -> int:
                return 1

    a = PossiblyNotCallable()
    result = a()  # error: "Object of type `PossiblyNotCallable` is not callable (possibly missing `__call__` method)"
    reveal_type(result)  # revealed: int
```

## Possibly unbound callable

```py
def _(flag: bool):
    if flag:
        class PossiblyUnbound:
            def __call__(self) -> int:
                return 1

    # error: [possibly-unresolved-reference]
    a = PossiblyUnbound()
    reveal_type(a())  # revealed: int
```

## Non-callable `__call__`

```py
class NonCallable:
    __call__ = 1

a = NonCallable()
# error: [call-non-callable] "Object of type `NonCallable` is not callable"
reveal_type(a())  # revealed: Unknown
```

## Recursive `__call__`

An annotation that refers back to the same instance never reaches a callable signature. Calling the
instance reports an error instead of repeatedly expanding `__call__`.

```py
class C:
    __call__: "C"

C()()  # error: [call-non-callable] "Object of type `C` is not callable"
```

## Mutually recursive `__call__`

Following `__call__` through several classes can also return to the original instance type without
finding a signature.

```py
class A:
    __call__: "B"

class B:
    __call__: A

A()()  # error: [call-non-callable] "Object of type `A` is not callable"
B()()  # error: [call-non-callable] "Object of type `B` is not callable"
```

## Recursive `__call__` in a union

A callable alternative in a union still contributes its return type, but does not make the recursive
alternative callable.

```py
from typing import Callable

class C:
    __call__: "C | Callable[[], int]"

# error: [call-non-callable]
reveal_type(C()())  # revealed: Unknown | int
```

## Recursive `__call__` compatibility

Checking compatibility with `Callable` also follows `__call__`, even without a call expression. A
recursive annotation provides no callable signature, including when it belongs to a protocol.

```py
from typing import Callable, Protocol

class C:
    __call__: "C"

class P(Protocol):
    __call__: "P"

def check(c: C, p: P):
    f: Callable[[], int] = c  # error: [invalid-assignment]
    g: Callable[[], int] = p  # error: [invalid-assignment]
    p()  # error: [call-non-callable]
```

## Recursive `__call__` through a classmethod

Binding a classmethod does not break a cycle through the wrapped instance's `__call__`. Both calling
the instance and checking its compatibility with `Callable` report errors.

```py
from typing import Callable, cast

class C:
    __call__ = classmethod(cast("C", object()))  # error: [invalid-argument-type]

def check(c: C):
    c()  # error: [call-non-callable]
    callback: Callable[[], int] = c  # error: [invalid-assignment]
```

## Recursive `__call__` through an enum intersection

Excluding one enum member does not break a recursive `__call__` annotation: the remaining members
still have the enum's instance type when looking up their call signature.

```py
from enum import Enum
from typing import Callable, Literal
from ty_extensions import Intersection, Not

class C(Enum):
    A = 1
    B = 2
    __call__: "Intersection[C, Not[Literal[C.A]]]"

def check(c: C):
    callback: Callable[[], int] = c  # error: [invalid-assignment]
    c()  # error: [call-non-callable]
```

## Possibly non-callable `__call__`

```py
def _(flag: bool):
    class NonCallable:
        if flag:
            __call__ = 1
        else:
            def __call__(self) -> int:
                return 1

    a = NonCallable()
    # error: [call-non-callable] "Object of type `Literal[1]` is not callable"
    reveal_type(a())  # revealed: Unknown | int
```

## Call binding errors

### Wrong argument type

```py
class C:
    def __call__(self, x: int) -> int:
        return 1

c = C()

# error: 15 [invalid-argument-type] "Argument to bound method `C.__call__` is incorrect: Expected `int`, found `Literal["foo"]`"
reveal_type(c("foo"))  # revealed: int
```

### Wrong argument type on `self`

```py
class C:
    # TODO this definition should also be an error; `C` must be assignable to type of `self`
    def __call__(self: int) -> int:
        return 1

c = C()

# error: 13 [invalid-argument-type] "Argument to bound method `C.__call__` is incorrect: Expected `int`, found `C`"
reveal_type(c())  # revealed: int
```

## Union over callables

### Possibly missing `__call__`

```py
def outer(cond1: bool):
    class Test:
        if cond1:
            def __call__(self): ...

    class Other:
        def __call__(self): ...

    def inner(cond2: bool):
        if cond2:
            a = Test()
        else:
            a = Other()

        # error: [call-non-callable] "Object of type `Test` is not callable (possibly missing `__call__` method)"
        a()
```
