# `typing.override`

## Basics

Decorating a method with `typing.override` decorator is an explicit indication to a type checker
that the method is intended to override a method on a superclass. If the decorated method does not
in fact override anything, a type checker should report a diagnostic on that method.

<!-- snapshot-diagnostics -->

```pyi
from typing_extensions import override, Callable, TypeVar

def lossy_decorator(fn: Callable) -> Callable: ...

class A:
    @override
    def __repr__(self): ...  # fine: overrides `object.__repr__`

class Parent:
    def foo(self): ...

    @property
    def my_property1(self) -> int: ...

    @property
    def my_property2(self) -> int: ...

    baz = None

    @classmethod
    def class_method1(cls) -> int: ...

    @staticmethod
    def static_method1() -> int: ...

    @classmethod
    def class_method2(cls) -> int: ...

    @staticmethod
    def static_method2() -> int: ...

    @lossy_decorator
    def decorated_1(self): ...

    @lossy_decorator
    def decorated_2(self): ...

    @lossy_decorator
    def decorated_3(self): ...

class Child(Parent):
    @override
    def foo(self): ...  # fine: overrides `Parent.foo`

    @property
    @override
    def my_property1(self) -> int: ...  # fine: overrides `Parent.my_property1`

    @override
    @property
    def my_property2(self) -> int: ...  # fine: overrides `Parent.my_property2`

    @override
    def baz(self): ...  # fine: overrides `Parent.baz`

    @classmethod
    @override
    def class_method1(cls) -> int: ...  # fine: overrides `Parent.class_method1`

    @staticmethod
    @override
    def static_method1() -> int: ...  # fine: overrides `Parent.static_method1`

    @override
    @classmethod
    def class_method2(cls) -> int: ...  # fine: overrides `Parent.class_method2`

    @override
    @staticmethod
    def static_method2() -> int: ...  # fine: overrides `Parent.static_method2`

    @override
    def decorated_1(self): ...  # fine: overrides `Parent.decorated_1`

    @override
    @lossy_decorator
    def decorated_2(self): ...  # fine: overrides `Parent.decorated_2`

    @lossy_decorator
    @override
    def decorated_3(self): ...  # fine: overrides `Parent.decorated_3`

class OtherChild(Parent): ...

class Grandchild(OtherChild):
    @override
    def foo(self): ...  # fine: overrides `Parent.foo`

    @override
    @property
    def my_property1(self) -> int: ...  # fine: overrides `Parent.my_property1`

    @override
    def baz(self): ...  # fine: overrides `Parent.baz`

    @classmethod
    @override
    def class_method1(cls) -> int: ...  # fine: overrides `Parent.class_method1`

    @staticmethod
    @override
    def static_method1() -> int: ...  # fine: overrides `Parent.static_method1`

    @override
    @classmethod
    def class_method2(cls) -> int: ...  # fine: overrides `Parent.class_method2`

    @override
    @staticmethod
    def static_method2() -> int: ...  # fine: overrides `Parent.static_method2`

    @override
    def decorated_1(self): ...  # fine: overrides `Parent.decorated_1`

    @override
    @lossy_decorator
    def decorated_2(self): ...  # fine: overrides `Parent.decorated_2`

    @lossy_decorator
    @override
    def decorated_3(self): ...  # fine: overrides `Parent.decorated_3`

class Invalid:
    @override
    def ___reprrr__(self): ...  # error: [invalid-explicit-override]

    @override
    @classmethod
    def foo(self): ...  # error: [invalid-explicit-override]

    @classmethod
    @override
    def bar(self): ...  # error: [invalid-explicit-override]

    @staticmethod
    @override
    def baz(): ...  # error: [invalid-explicit-override]

    @override
    @staticmethod
    def eggs(): ...  # error: [invalid-explicit-override]

    @property
    @override
    def bad_property1(self) -> int: ...  # error: [invalid-explicit-override]

    @override
    @property
    def bad_property2(self) -> int: ...  # error: [invalid-explicit-override]

    @property
    @override
    def bad_settable_property(self) -> int: ...  # error: [invalid-explicit-override]
    @bad_settable_property.setter
    def bad_settable_property(self, x: int) -> None: ...

    @lossy_decorator
    @override
    def lossy(self): ...  # TODO: should emit `invalid-explicit-override` here

    @override
    @lossy_decorator
    def lossy2(self): ...  # TODO: should emit `invalid-explicit-override` here

# TODO: all overrides in this class should cause us to emit *Liskov* violations,
# but not `@override` violations
class LiskovViolatingButNotOverrideViolating(Parent):
    @override
    @property
    def foo(self) -> int: ...

    @override
    def my_property1(self) -> int: ...

    # TODO: This maybe shouldn't be a Liskov violation? Whether called on the type or
    # on an instance, it will behave the same from the caller's perspective. The only difference
    # is whether the method body gets access to `cls`, which is not a concern of Liskov.
    @staticmethod
    @override
    def class_method1() -> int: ...  # error: [invalid-method-override]

    @classmethod
    @override
    def static_method1(cls) -> int: ...

# Diagnostic edge case: `override` is very far away from the method definition in the source code:

T = TypeVar("T")

def identity(x: T) -> T: ...

class Foo:
    @override
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
    def bar(self): ...  # error: [invalid-explicit-override]
```

## Possibly-unbound definitions

```py
from typing_extensions import override

def coinflip() -> bool:
    return False

class Parent:
    if coinflip():
        def method1(self) -> None: ...
        def method2(self) -> None: ...

    if coinflip():
        def method3(self) -> None: ...
        def method4(self) -> None: ...
    else:
        def method3(self) -> None: ...
        def method4(self) -> None: ...

    def method5(self) -> None: ...
    def method6(self) -> None: ...

class Child(Parent):
    @override
    def method1(self) -> None: ...
    @override
    def method2(self) -> None: ...

    if coinflip():
        @override
        def method3(self) -> None: ...

    if coinflip():
        @override
        def method4(self) -> None: ...
    else:
        @override
        def method4(self) -> None: ...

    if coinflip():
        @override
        def method5(self) -> None: ...

    if coinflip():
        @override
        def method6(self) -> None: ...
    else:
        @override
        def method6(self) -> None: ...

    if coinflip():
        @override
        def method7(self) -> None: ...  # error: [invalid-explicit-override]

    if coinflip():
        @override
        def method8(self) -> None: ...  # error: [invalid-explicit-override]
    else:
        @override
        def method8(self) -> None: ...
```

## Multiple reachable definitions, only one of which is decorated with `@override`

The diagnostic should point to the first definition decorated with `@override`, which may not
necessarily be the first definition of the symbol overall:

`runtime.py`:

```py
from typing_extensions import override, overload

def coinflip() -> bool:
    return True

class Foo:
    if coinflip():
        def method(self, x): ...
    elif coinflip():
        @overload
        def method(self, x: str) -> str: ...
        @overload
        def method(self, x: int) -> int: ...
        @override
        def method(self, x: str | int) -> str | int:  # error: [invalid-explicit-override]
            return x
    elif coinflip():
        @override
        def method(self, x): ...
```

stub.pyi\`:

```pyi
from typing_extensions import override, overload

def coinflip() -> bool:
    return True

class Foo:
    if coinflip():
        def method(self, x): ...
    elif coinflip():
        @overload
        @override
        def method(self, x: str) -> str: ...  # error: [invalid-explicit-override]
        @overload
        def method(self, x: int) -> int: ...

    if coinflip():
        def method2(self, x): ...
    elif coinflip():
        @overload
        @override
        def method2(self, x: str) -> str: ...
        @overload
        def method2(self, x: int) -> int: ...
    else:
       # TODO: not sure why this is being emitted on this line rather than on
       # the first overload in the `elif` block? Ideally it would be emitted
       # on the first reachable definition, but perhaps this is due to the way
       # name lookups are deferred in stub files...? -- AW
       @override
       def method2(self, x): ...  # error: [invalid-explicit-override]
```

## Definitions in statically known branches

```toml
[environment]
python-version = "3.10"
```

```py
import sys
from typing_extensions import override, overload

class Parent:
    if sys.version_info >= (3, 10):
        def foo(self) -> None: ...
        def foooo(self) -> None: ...
    else:
        def bar(self) -> None: ...
        def baz(self) -> None: ...
        def spam(self) -> None: ...

class Child(Parent):
    @override
    def foo(self) -> None: ...

    # The declaration on `Parent` is not reachable,
    # so this is an error
    @override
    def bar(self) -> None: ...  # error: [invalid-explicit-override]

    if sys.version_info >= (3, 10):
        @override
        def foooo(self) -> None: ...
        @override
        def baz(self) -> None: ...  # error: [invalid-explicit-override]
    else:
        # This doesn't override any reachable definitions,
        # but the subclass definition also isn't a reachable definition
        # from the end of the scope with the given configuration,
        # so it's not flagged
        @override
        def foooo(self) -> None: ...
        @override
        def spam(self) -> None: ...
```

## Overloads

The typing spec states that for an overloaded method, `@override` should only be applied to the
implementation function. However, we nonetheless respect the decorator in this situation, even
though we also emit `invalid-overload` on these methods.

```py
from typing_extensions import override, overload

class Spam:
    @overload
    def foo(self, x: str) -> str: ...
    @overload
    def foo(self, x: int) -> int: ...
    @override
    def foo(self, x: str | int) -> str | int:  # error: [invalid-explicit-override]
        return x

    @overload
    @override
    def bar(self, x: str) -> str: ...
    @overload
    @override
    def bar(self, x: int) -> int: ...
    @override
    # error: [invalid-overload] "`@override` decorator should be applied only to the overload implementation"
    # error: [invalid-overload] "`@override` decorator should be applied only to the overload implementation"
    # error: [invalid-explicit-override]
    def bar(self, x: str | int) -> str | int:
        return x

    @overload
    @override
    def baz(self, x: str) -> str: ...
    @overload
    def baz(self, x: int) -> int: ...

    # error: [invalid-overload] "`@override` decorator should be applied only to the overload implementation"
    # error: [invalid-explicit-override]
    def baz(self, x: str | int) -> str | int:
        return x
```

In a stub file, `@override` should always be applied to the first overload. Even if it isn't, we
always emit `invalid-explicit-override` diagnostics on the first overload.

`module.pyi`:

```pyi
from typing_extensions import override, overload

class Spam:
    @overload
    def foo(self, x: str) -> str: ...  # error: [invalid-explicit-override]
    @overload
    @override
    # error: [invalid-overload]  "`@override` decorator should be applied only to the first overload"
    def foo(self, x: int) -> int: ...

    @overload
    @override
    def bar(self, x: str) -> str: ...  # error: [invalid-explicit-override]
    @overload
    @override
    # error: [invalid-overload]  "`@override` decorator should be applied only to the first overload"
    def bar(self, x: int) -> int: ...

    @overload
    @override
    def baz(self, x: str) -> str: ...  # error: [invalid-explicit-override]
    @overload
    def baz(self, x: int) -> int: ...
```

## Overloads in statically-known branches in stub files

```toml
[environment]
python-version = "3.10"
```

```pyi
import sys
from typing_extensions import overload, override

class Foo:
    if sys.version_info >= (3, 10):
        @overload
        @override
        def method(self, x: int) -> int: ...  # error: [invalid-explicit-override]
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
        @override
        def method2(self, x: int) -> int: ...
    @overload
    def method2(self, x: str) -> str: ...
```

## Classes inheriting from `Any`

```py
from typing_extensions import Any, override
from does_not_exist import SomethingUnknown  # error: [unresolved-import]

class Parent1(Any): ...
class Parent2(SomethingUnknown): ...

class Child1(Parent1):
    @override
    def bar(self): ...  # fine

class Child2(Parent2):
    @override
    def bar(self): ...  # fine
```

## Override of a synthesized method

```pyi
from typing_extensions import NamedTuple, TypedDict, override, Any, Self
from dataclasses import dataclass

@dataclass(order=True)
class ParentDataclass:
    x: int

class Child(ParentDataclass):
    @override
    def __lt__(self, other: ParentDataclass) -> bool: ...  # fine

class MyNamedTuple(NamedTuple):
    x: int

    @override
    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_asdict`"
    def _asdict(self, /) -> dict[str, Any]: ...

class MyNamedTupleParent(NamedTuple):
    x: int

class MyNamedTupleChild(MyNamedTupleParent):
    @override
    def _asdict(self, /) -> dict[str, Any]: ...  # fine

class MyTypedDict(TypedDict):
    x: int

    @override
    # TODO: it's invalid to define a method on a `TypedDict` class,
    # so we should emit a diagnostic here.
    # It shouldn't be an `invalid-explicit-override` diagnostic, however.
    def copy(self) -> Self: ...

class Grandparent(Any): ...

class Parent(Grandparent, NamedTuple):  # error: [invalid-named-tuple]
    x: int

class Child(Parent):
    @override
    def foo(self): ...  # fine because `Any` is in the MRO
```
