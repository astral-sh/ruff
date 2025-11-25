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
    def bar(self) -> int: ...  # fine: overrides `Parent.bar`
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
    def ___reprrr__(self): ...  # error: [explicit-override]
    @override
    @classmethod
    def foo(self): ...  # error: [explicit-override]
    @classmethod
    @override
    def bar(self): ...  # error: [explicit-override]
    @staticmethod
    @override
    def baz(): ...  # error: [explicit-override]
    @override
    @staticmethod
    def eggs(): ...  # error: [explicit-override]
    @property
    @override
    def bad_property1(self) -> int: ...  # TODO: should emit `invalid-override` here
    @override
    @property
    def bad_property2(self) -> int: ...  # TODO: should emit `invalid-override` here
    @lossy_decorator
    @override
    def lossy(self): ...  # TODO: should emit `invalid-override` here
    @override
    @lossy_decorator
    def lossy2(self): ...  # TODO: should emit `invalid-override` here

# TODO: all overrides in this class should cause us to emit *Liskov* violations,
# but not `@override` violations
class LiskovViolatingButNotOverrideViolating(Parent):
    @override
    @property
    def foo(self) -> int: ...
    @override
    def my_property1(self) -> int: ...
    @staticmethod
    @override
    def class_method1() -> int: ...
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
    def bar(self): ...  # error: [explicit-override]
```

## Overloads

The typing spec states that for an overloaded method, `@override` should only be applied to the
implementation function. However, we nonetheless respect the decorator in this situation, even =
though we also emit `invalid-overload` on these methods.

```py
from typing_extensions import override, overload

class Spam:
    @overload
    def foo(self, x: str) -> str: ...
    @overload
    def foo(self, x: int) -> int: ...
    @override
    def foo(self, x: str | int) -> str | int:  # error: [explicit-override]
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
    # error: [explicit-override]
    def bar(self, x: str | int) -> str | int:
        return x

    @overload
    @override
    def baz(self, x: str) -> str: ...
    @overload
    def baz(self, x: int) -> int: ...
    # error: [invalid-overload] "`@override` decorator should be applied only to the overload implementation"
    # error: [explicit-override]
    def baz(self, x: str | int) -> str | int:
        return x
```

In a stub file, `@override` should always be applied to the first overload. Even if it isn't, we
always emit `explicit-override` diagnostics on the first overload.

`module.pyi`:

```pyi
from typing_extensions import override, overload

class Spam:
    @overload
    def foo(self, x: str) -> str: ...  # error: [explicit-override]
    @overload
    @override
    # error: [invalid-overload]  "`@override` decorator should be applied only to the first overload"
    def foo(self, x: int) -> int: ...

    @overload
    @override
    def bar(self, x: str) -> str: ...  # error: [explicit-override]
    @overload
    @override
    # error: [invalid-overload]  "`@override` decorator should be applied only to the first overload"
    def bar(self, x: int) -> int: ...

    @overload
    @override
    def baz(self, x: str) -> str: ...  # error: [explicit-override]
    @overload
    def baz(self, x: int) -> int: ...
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
