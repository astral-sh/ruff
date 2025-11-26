# Tests for the `@typing(_extensions).final` decorator

## Cannot subclass a class decorated with `@final`

Don't do this:

```py
import typing_extensions
from typing import final

@final
class A: ...

class B(A): ...  # error: 9 [subclass-of-final-class] "Class `B` cannot inherit from final class `A`"

@typing_extensions.final
class C: ...

class D(C): ...  # error: [subclass-of-final-class]
class E: ...
class F: ...
class G: ...

# fmt: off
class H(
    E,
    F,
    A,  # error: [subclass-of-final-class]
    G,
): ...
```

## Cannot override a method decorated with `@final`

<!-- snapshot-diagnostics -->

```pyi
from typing_extensions import final, Callable, TypeVar

def lossy_decorator(fn: Callable) -> Callable: ...

class Parent:
    @final
    def foo(self): ...
    @final
    @property
    def my_property1(self) -> int: ...
    @property
    @final
    def my_property2(self) -> int: ...
    @final
    @classmethod
    def class_method1(cls) -> int: ...
    @staticmethod
    @final
    def static_method1() -> int: ...
    @final
    @classmethod
    def class_method2(cls) -> int: ...
    @staticmethod
    @final
    def static_method2() -> int: ...
    @lossy_decorator
    @final
    def decorated_1(self): ...
    @final
    @lossy_decorator
    def decorated_2(self): ...

class Child(Parent):
    def foo(self): ...  # error: [override-of-final-method]
    @property
    def my_property1(self) -> int: ...  # error: [override-of-final-method]
    @property
    def my_property2(self) -> int: ...  # error: [override-of-final-method]
    @classmethod
    def class_method1(cls) -> int: ...  # error: [override-of-final-method]
    @staticmethod
    def static_method1() -> int: ...  # error: [override-of-final-method]
    @classmethod
    def class_method2(cls) -> int: ...  # error: [override-of-final-method]
    @staticmethod
    def static_method2() -> int: ...  # error: [override-of-final-method]
    def decorated_1(self): ...  # TODO: should emit [override-of-final-method]
    @lossy_decorator
    def decorated_2(self): ...  # TODO: should emit [override-of-final-method]

class OtherChild(Parent): ...

class Grandchild(OtherChild):
    @staticmethod
    # TODO: we should emit a Liskov violation here too
    # error: [override-of-final-method]
    def foo(): ...
    @property
    # TODO: we should emit a Liskov violation here too
    # error: [override-of-final-method]
    def my_property1(self) -> str: ...
    # TODO: we should emit a Liskov violation here too
    # error: [override-of-final-method]
    class_method1 = None

# Diagnostic edge case: `final` is very far away from the method definition in the source code:

T = TypeVar("T")

def identity(x: T) -> T: ...

class Foo:
    @final
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    @identity
    def bar(self): ...

class Baz(Foo):
    def bar(self): ...  # error: [override-of-final-method]
```

## Diagnostic edge case: superclass with `@final` method has the same name as the subclass

<!-- snapshot-diagnostics -->

`module1.py`:

```py
from typing import final

class Foo:
    @final
    def f(self): ...
```

`module2.py`:

```py
import module1

class Foo(module1.Foo):
    def f(self): ...  # error: [override-of-final-method]
```

## Overloaded methods decorated with `@final`

In a stub file, `@final` should be applied to the first overload. In a runtime file, `@final` should
only be applied to the implementation function.

<!-- snapshot-diagnostics -->

`stub.pyi`:

```pyi
from typing import final, overload

class Good:
    @overload
    @final
    def bar(self, x: str) -> str: ...
    @overload
    def bar(self, x: int) -> int: ...
    @final
    @overload
    def baz(self, x: str) -> str: ...
    @overload
    def baz(self, x: int) -> int: ...

class ChildOfGood(Good):
    @overload
    def bar(self, x: str) -> str: ...
    @overload
    def bar(self, x: int) -> int: ...  # error: [override-of-final-method]
    @overload
    def baz(self, x: str) -> str: ...
    @overload
    def baz(self, x: int) -> int: ...  # error: [override-of-final-method]

class Bad:
    @overload
    def bar(self, x: str) -> str: ...
    @overload
    @final
    # error: [invalid-overload]
    def bar(self, x: int) -> int: ...
    @overload
    def baz(self, x: str) -> str: ...
    @final
    @overload
    # error: [invalid-overload]
    def baz(self, x: int) -> int: ...

class ChildOfBad(Bad):
    @overload
    def bar(self, x: str) -> str: ...
    @overload
    def bar(self, x: int) -> int: ...  # error: [override-of-final-method]
    @overload
    def baz(self, x: str) -> str: ...
    @overload
    def baz(self, x: int) -> int: ...  # error: [override-of-final-method]
```

`main.py`:

```py
from typing import overload, final

class Good:
    @overload
    def f(self, x: str) -> str: ...
    @overload
    def f(self, x: int) -> int: ...
    @final
    def f(self, x: int | str) -> int | str:
        return x

class ChildOfGood(Good):
    @overload
    def f(self, x: str) -> str: ...
    @overload
    def f(self, x: int) -> int: ...
    # error: [override-of-final-method]
    def f(self, x: int | str) -> int | str:
        return x

class Bad:
    @overload
    @final
    def f(self, x: str) -> str: ...
    @overload
    def f(self, x: int) -> int: ...
    # error: [invalid-overload]
    def f(self, x: int | str) -> int | str:
        return x

    @final
    @overload
    def g(self, x: str) -> str: ...
    @overload
    def g(self, x: int) -> int: ...
    # error: [invalid-overload]
    def g(self, x: int | str) -> int | str:
        return x

    @overload
    def h(self, x: str) -> str: ...
    @overload
    @final
    def h(self, x: int) -> int: ...
    # error: [invalid-overload]
    def h(self, x: int | str) -> int | str:
        return x

    @overload
    def i(self, x: str) -> str: ...
    @final
    @overload
    def i(self, x: int) -> int: ...
    # error: [invalid-overload]
    def i(self, x: int | str) -> int | str:
        return x

class ChildOfBad(Bad):
    # TODO: these should all cause us to emit Liskov violations as well
    f = None  # error: [override-of-final-method]
    g = None  # error: [override-of-final-method]
    h = None  # error: [override-of-final-method]
    i = None  # error: [override-of-final-method]
```

## Edge case: the function is decorated with `@final` but originally defined elsewhere

As of 2025-11-26, pyrefly emits a diagnostic on this, but mypy and pyright do not:

```py
from typing import final

class A:
    @final
    def method(self): ...

class B:
    method = A.method

class C(B):
    def method(self): ...  # no diagnostic
```
