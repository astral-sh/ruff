# Scoping rules for type variables

```toml
[environment]
python-version = "3.12"
```

Most of these tests come from the [Scoping rules for type variables][scoping] section of the typing
spec.

## Typevar used outside of generic function or class

Typevars may only be used in generic function or class definitions.

```py
from typing import TypeVar

T = TypeVar("T")

# TODO: error
x: T

class C:
    # TODO: error
    x: T

def f() -> None:
    # TODO: error
    x: T
```

## Legacy typevar used multiple times

> A type variable used in a generic function could be inferred to represent different types in the
> same code block.

This only applies to typevars defined using the legacy syntax, since the PEP 695 syntax creates a
new distinct typevar for each occurrence.

```py
from typing import TypeVar

T = TypeVar("T")

def f1(x: T) -> T:
    return x

def f2(x: T) -> T:
    return x

f1(1)
f2("a")
```

## Typevar inferred multiple times

> A type variable used in a generic function could be inferred to represent different types in the
> same code block.

This also applies to a single generic function being used multiple times, instantiating the typevar
to a different type each time.

```py
def f[T](x: T) -> T:
    return x

reveal_type(f(1))  # revealed: Literal[1]
reveal_type(f("a"))  # revealed: Literal["a"]
```

## Methods can mention class typevars

> A type variable used in a method of a generic class that coincides with one of the variables that
> parameterize this class is always bound to that variable.

```py
class C[T]:
    def m1(self, x: T) -> T:
        return x

    def m2(self, x: T) -> T:
        return x

c: C[int] = C[int]()
c.m1(1)
c.m2(1)
# error: [invalid-argument-type] "Argument to bound method `m2` is incorrect: Expected `int`, found `Literal["string"]`"
c.m2("string")
```

## Functions on generic classes are descriptors

This repeats the tests in the [Functions as descriptors](./call/methods.md) test suite, but on a
generic class. This ensures that we are carrying any specializations through the entirety of the
descriptor protocol, which is how `self` parameters are bound to instance methods.

```py
from inspect import getattr_static

class C[T]:
    def f(self, x: T) -> str:
        return "a"

reveal_type(getattr_static(C[int], "f"))  # revealed: def f(self, x: int) -> str
reveal_type(getattr_static(C[int], "f").__get__)  # revealed: <method-wrapper `__get__` of `f`>
reveal_type(getattr_static(C[int], "f").__get__(None, C[int]))  # revealed: def f(self, x: int) -> str
# revealed: bound method C[int].f(x: int) -> str
reveal_type(getattr_static(C[int], "f").__get__(C[int](), C[int]))

reveal_type(C[int].f)  # revealed: def f(self, x: int) -> str
reveal_type(C[int]().f)  # revealed: bound method C[int].f(x: int) -> str

bound_method = C[int]().f
reveal_type(bound_method.__self__)  # revealed: C[int]
reveal_type(bound_method.__func__)  # revealed: def f(self, x: int) -> str

reveal_type(C[int]().f(1))  # revealed: str
reveal_type(bound_method(1))  # revealed: str

C[int].f(1)  # error: [missing-argument]
reveal_type(C[int].f(C[int](), 1))  # revealed: str

class D[U](C[U]):
    pass

reveal_type(D[int]().f)  # revealed: bound method D[int].f(x: int) -> str
```

## Methods can mention other typevars

> A type variable used in a method that does not match any of the variables that parameterize the
> class makes this method a generic function in that variable.

```py
from typing import TypeVar, Generic

T = TypeVar("T")
S = TypeVar("S")

class Legacy(Generic[T]):
    def m(self, x: T, y: S) -> S:
        return y

legacy: Legacy[int] = Legacy()
reveal_type(legacy.m(1, "string"))  # revealed: Literal["string"]
```

With PEP 695 syntax, it is clearer that the method uses a separate typevar:

```py
class C[T]:
    def m[S](self, x: T, y: S) -> S:
        return y

c: C[int] = C()
reveal_type(c.m(1, "string"))  # revealed: Literal["string"]
```

## Unbound typevars

> Unbound type variables should not appear in the bodies of generic functions, or in the class
> bodies apart from method definitions.

This is true with the legacy syntax:

```py
from typing import TypeVar, Generic

T = TypeVar("T")
S = TypeVar("S")

def f(x: T) -> None:
    x: list[T] = []
    # TODO: invalid-assignment error
    y: list[S] = []

class C(Generic[T]):
    # TODO: error: cannot use S if it's not in the current generic context
    x: list[S] = []

    # This is not an error, as shown in the previous test
    def m(self, x: S) -> S:
        return x
```

This is true with PEP 695 syntax, as well, though we must use the legacy syntax to define the
unbound typevars:

`pep695.py`:

```py
from typing import TypeVar

S = TypeVar("S")

def f[T](x: T) -> None:
    x: list[T] = []
    # TODO: invalid assignment error
    y: list[S] = []

class C[T]:
    # TODO: error: cannot use S if it's not in the current generic context
    x: list[S] = []

    def m1(self, x: S) -> S:
        return x

    def m2[S](self, x: S) -> S:
        return x
```

## Nested formal typevars must be distinct

Generic functions and classes can be nested in each other, but it is an error for the same typevar
to be used in nested generic definitions.

Note that the typing spec only mentions two specific versions of this rule:

> A generic class definition that appears inside a generic function should not use type variables
> that parameterize the generic function.

and

> A generic class nested in another generic class cannot use the same type variables.

We assume that the more general form holds.

### Generic function within generic function

```py
def f[T](x: T, y: T) -> None:
    def ok[S](a: S, b: S) -> None: ...

    # TODO: error
    def bad[T](a: T, b: T) -> None: ...
```

### Generic method within generic class

```py
class C[T]:
    def ok[S](self, a: S, b: S) -> None: ...

    # TODO: error
    def bad[T](self, a: T, b: T) -> None: ...
```

### Generic class within generic function

```py
from typing import Iterable

def f[T](x: T, y: T) -> None:
    class Ok[S]: ...
    # TODO: error for reuse of typevar
    class Bad1[T]: ...
    # TODO: error for reuse of typevar
    class Bad2(Iterable[T]): ...
```

### Generic class within generic class

```py
from typing import Iterable

class C[T]:
    class Ok1[S]: ...
    # TODO: error for reuse of typevar
    class Bad1[T]: ...
    # TODO: error for reuse of typevar
    class Bad2(Iterable[T]): ...
```

## Class scopes do not cover inner scopes

Just like regular symbols, the typevars of a generic class are only available in that class's scope,
and are not available in nested scopes.

```py
class C[T]:
    ok1: list[T] = []

    class Bad:
        # TODO: error: cannot refer to T in nested scope
        bad: list[T] = []

    class Inner[S]: ...
    ok2: Inner[T]
```

[scoping]: https://typing.python.org/en/latest/spec/generics.html#scoping-rules-for-type-variables
