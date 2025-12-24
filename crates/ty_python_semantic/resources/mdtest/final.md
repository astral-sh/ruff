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

    @property
    @final
    def my_property3(self) -> int: ...

    @final
    @classmethod
    def class_method1(cls) -> int: ...

    @classmethod
    @final
    def class_method2(cls) -> int: ...

    @final
    @staticmethod
    def static_method1() -> int: ...

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
    # explicitly test the concise diagnostic message,
    # which is different to the verbose diagnostic summary message:
    #
    # error: [override-of-final-method] "Cannot override final member `foo` from superclass `Parent`"
    def foo(self): ...
    @property
    def my_property1(self) -> int: ...  # error: [override-of-final-method]

    @property
    def my_property2(self) -> int: ...  # error: [override-of-final-method]
    @my_property2.setter
    def my_property2(self, x: int) -> None: ...

    @property
    def my_property3(self) -> int: ...  # error: [override-of-final-method]
    @my_property3.deleter
    def my_proeprty3(self) -> None: ...

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
    # TODO: The Liskov violation here maybe shouldn't be emitted? Whether called on the
    # type or on an instance, it will behave the same from the caller's perspective. The only
    # difference is whether the method body gets access to `self`, which is not a
    # concern of Liskov.
    @staticmethod
    # error: [override-of-final-method]
    # error: [invalid-method-override]
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

As of 2025-11-26, pyrefly emits a diagnostic on this, but mypy and pyright do not. For mypy and
pyright to emit a diagnostic, the superclass definition decorated with `@final` must be a literal
function definition: an assignment definition where the right-hand side of the assignment is a
`@final-decorated` function is not sufficient for them to consider the superclass definition as
being `@final`.

For now, we choose to follow mypy's and pyright's behaviour here, in order to maximise compatibility
with other type checkers. We may decide to change this in the future, however, as it would simplify
our implementation. Mypy's and pyright's behaviour here is also arguably inconsistent with their
treatment of other type qualifiers such as `Final`. As discussed in
<https://discuss.python.org/t/imported-final-variable/82429>, both type checkers view the `Final`
type qualifier as travelling *across* scopes.

```py
from typing import final

class A:
    @final
    def method(self) -> None: ...

class B:
    method = A.method

class C(B):
    def method(self) -> None: ...  # no diagnostic here (see prose discussion above)
```

## Constructor methods are also checked

```py
from typing import final

class A:
    @final
    def __init__(self) -> None: ...

class B(A):
    def __init__(self) -> None: ...  # error: [override-of-final-method]
```

## Only the first `@final` violation is reported

(Don't do this.)

<!-- snapshot-diagnostics -->

```py
from typing import final

class A:
    @final
    def f(self): ...

class B(A):
    @final
    def f(self): ...  # error: [override-of-final-method]

class C(B):
    @final
    # we only emit one error here, not two
    def f(self): ...  # error: [override-of-final-method]
```

## For when you just really want to drive the point home

```py
from typing import final, Final

@final
@final
@final
@final
@final
@final
class A:
    @final
    @final
    @final
    @final
    @final
    def method(self): ...

@final
@final
@final
@final
@final
class B:
    method: Final = A.method

class C(A):  # error: [subclass-of-final-class]
    def method(self): ...  # error: [override-of-final-method]

class D(B):  # error: [subclass-of-final-class]
    # TODO: we should emit a diagnostic here
    def method(self): ...
```

## An `@final` method is overridden by an implicit instance attribute

```py
from typing import final, Any

class Parent:
    @final
    def method(self) -> None: ...

class Child(Parent):
    def __init__(self) -> None:
        self.method: Any = 42  # TODO: we should emit `[override-of-final-method]` here
```

## A possibly-undefined `@final` method is overridden

<!-- snapshot-diagnostics -->

```py
from typing import final

def coinflip() -> bool:
    return False

class A:
    if coinflip():
        @final
        def method1(self) -> None: ...
    else:
        def method1(self) -> None: ...

    if coinflip():
        def method2(self) -> None: ...
    else:
        @final
        def method2(self) -> None: ...

    if coinflip():
        @final
        def method3(self) -> None: ...
    else:
        @final
        def method3(self) -> None: ...

    if coinflip():
        def method4(self) -> None: ...
    elif coinflip():
        @final
        def method4(self) -> None: ...
    else:
        def method4(self) -> None: ...

class B(A):
    def method1(self) -> None: ...  # error: [override-of-final-method]
    def method2(self) -> None: ...  # error: [override-of-final-method]
    def method3(self) -> None: ...  # error: [override-of-final-method]

    # check that autofixes don't introduce invalid syntax
    # if there are multiple statements on one line
    #
    # TODO: we should emit a Liskov violation here too
    # error: [override-of-final-method]
    method4 = 42
    unrelated = 56  # fmt: skip

# Possible overrides of possibly `@final` methods...
class C(A):
    if coinflip():
        def method1(self) -> None: ...  # error: [override-of-final-method]
    else:
        pass

    if coinflip():
        def method2(self) -> None: ...  # error: [override-of-final-method]
    else:
        def method2(self) -> None: ...

    if coinflip():
        def method3(self) -> None: ...  # error: [override-of-final-method]

    # TODO: we should emit Liskov violations here too:
    if coinflip():
        method4 = 42  # error: [override-of-final-method]
    else:
        method4 = 56
```

## Definitions in statically known branches

```toml
[environment]
python-version = "3.10"
```

```py
import sys
from typing_extensions import final

class Parent:
    if sys.version_info >= (3, 10):
        @final
        def foo(self) -> None: ...
        @final
        def foooo(self) -> None: ...
        @final
        def baaaaar(self) -> None: ...
    else:
        @final
        def bar(self) -> None: ...
        @final
        def baz(self) -> None: ...
        @final
        def spam(self) -> None: ...

class Child(Parent):
    def foo(self) -> None: ...  # error: [override-of-final-method]

    # The declaration on `Parent` is not reachable,
    # so this is fine
    def bar(self) -> None: ...

    if sys.version_info >= (3, 10):
        def foooo(self) -> None: ...  # error: [override-of-final-method]
        def baz(self) -> None: ...
    else:
        # Fine because this doesn't override any reachable definitions
        def foooo(self) -> None: ...

        # There are `@final` definitions being overridden here,
        # but the definitions that override them are unreachable
        def spam(self) -> None: ...
        def baaaaar(self) -> None: ...
```

## Overloads in statically-known branches in stub files

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.10"
```

```pyi
import sys
from typing_extensions import overload, final

class Foo:
    if sys.version_info >= (3, 10):
        @overload
        @final
        def method(self, x: int) -> int: ...
    else:
        @overload
        def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...

    if sys.version_info >= (3, 10):
        @overload
        def method2(self, x: int) -> int: ...
    else:
        @overload
        @final
        def method2(self, x: int) -> int: ...
    @overload
    def method2(self, x: str) -> str: ...

class Bar(Foo):
    @overload
    def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...  # error: [override-of-final-method]

    # This is fine: the only overload that is marked `@final`
    # is in a statically-unreachable branch
    @overload
    def method2(self, x: int) -> int: ...
    @overload
    def method2(self, x: str) -> str: ...
```
