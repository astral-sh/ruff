# `typing.override`

## Basics

Decorating a method with `typing.override` decorator is an explicit indication to a type checker
that the method is intended to override a method on a superclass. If the decorated method does not
in fact override anything, a type checker should report a diagnostic on that method.

<!-- snapshot-diagnostics -->

```pyi
from typing_extensions import Any, Callable, TypeVar, override

# Decorator intentionally erases the wrapped signature.
def lossy_decorator(fn: Callable[..., Any]) -> Callable[..., Any]: ...

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
    def lossy(self): ...  # error: [invalid-explicit-override]
    @override
    @lossy_decorator
    def lossy2(self): ...  # error: [invalid-explicit-override]

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

## Missing `@override` decorator

```toml
[rules]
missing-override-decorator = "error"
```

This rule requires the `@override` decorator on any method that overrides a superclass member, with
the exception of `__init__`, `__new__`, `__init_subclass__`, or `__post_init__`.

```py
from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing_extensions import Any, Protocol, overload, override

class Parent:
    attr = None

    def method(self) -> int:
        return 1

    @property
    def prop(self) -> int:
        return 1

    @classmethod
    def class_method(cls) -> int:
        return 1

    @staticmethod
    def static_method() -> int:
        return 1

    @overload
    def overloaded(self, value: int) -> int: ...
    @overload
    def overloaded(self, value: str) -> str: ...
    def overloaded(self, value: int | str) -> int | str:
        return value

class Child(Parent):
    def method(self) -> int:  # error: [missing-override-decorator]
        return 2

    @property
    def prop(self) -> int:  # error: [missing-override-decorator]
        return 2

    @classmethod
    def class_method(cls) -> int:  # error: [missing-override-decorator]
        return 2

    @staticmethod
    def static_method() -> int:  # error: [missing-override-decorator]
        return 2

class AttributeChild(Parent):
    def attr(self) -> None:  # error: [missing-override-decorator]
        pass

class OverloadChild(Parent):
    @overload
    def overloaded(self, value: int) -> int: ...
    @overload
    def overloaded(self, value: str) -> str: ...
    def overloaded(self, value: int | str) -> int | str:  # error: [missing-override-decorator]
        return value

# Implementing an interface-only protocol member requires `@override`.
class ProtocolInterface(Protocol):
    def method(self) -> int: ...

class ProtocolImplementation(ProtocolInterface):
    def method(self) -> int:  # error: [missing-override-decorator]
        return 1

# Implementing an abstract interface method requires `@override`.
class AbstractInterface(ABC):
    @abstractmethod
    def method(self) -> int: ...

class AbstractImplementation(AbstractInterface):
    def method(self) -> int:  # error: [missing-override-decorator]
        return 1

class ExplicitChild(Parent):
    @override
    def method(self) -> int:
        return 2

    @property
    @override
    def prop(self) -> int:
        return 2

    @override
    def attr(self) -> None:
        pass

    @overload
    def overloaded(self, value: int) -> int: ...
    @overload
    def overloaded(self, value: str) -> str: ...
    @override
    def overloaded(self, value: int | str) -> int | str:
        return value

class OverrideOnOverload(Parent):
    @overload
    @override
    def overloaded(self, value: int) -> int: ...  # error: [invalid-overload]
    @overload
    def overloaded(self, value: str) -> str: ...
    def overloaded(self, value: int | str) -> int | str:  # error: [missing-override-decorator]
        return value

class OverrideOnImplementation(Parent):
    @overload
    def overloaded(self, value: int) -> int: ...
    @overload
    def overloaded(self, value: str) -> str: ...
    @override
    def overloaded(self, value: int | str) -> int | str:
        return value

class ConstructorParent:
    def __init__(self, value: int) -> None:
        pass

    def __new__(cls, value: int) -> "ConstructorParent":
        raise NotImplementedError

    def __init_subclass__(cls, value: int = 0) -> None:
        pass

    def __post_init__(self) -> None:
        pass

class ConstructorChild(ConstructorParent):
    def __init__(self, value: str) -> None:
        pass

    def __new__(cls, value: str) -> "ConstructorChild":
        raise NotImplementedError

    def __init_subclass__(cls, value: str = "") -> None:
        pass

    def __post_init__(self) -> None:
        pass

# Overrides of ty-generated dataclass members require `@override`, except for constructor-like
# methods.
@dataclass(order=True)
class DataClassParent:
    field: int

class DataClassChild(DataClassParent):  # error: [subclass-of-dataclass-with-order]
    def __init__(self, field: str) -> None:
        self.field = 1

    def __lt__(self, other: DataClassParent) -> bool:  # error: [missing-override-decorator]
        return True

class DynamicParent(Any): ...

class DynamicChild(DynamicParent):
    def method(self) -> int:
        return 1

class SameFilePropertyParent:
    @property
    def prop(self) -> int:
        return 1

class SameFilePropertyChild(SameFilePropertyParent):
    @SameFilePropertyParent.prop.deleter
    def prop(self) -> None:  # error: [missing-override-decorator]
        pass
```

`base_property.py`:

```py
# This padding makes the inherited getter's AST index larger than the entire child module's AST,
# so attempting to resolve the getter in the (incorrect) context of the child module will induce a panic.
# This reproduces the bug reported in astral-sh/ty#3653.
_padding = (
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
)

class BaseProperty:
    @property
    def prop(self) -> int:
        return 1

    def method(self) -> int:
        return 1
```

`property_setter.py`:

```py
from typing_extensions import Callable, TypeVar, override

from base_property import BaseProperty

_T = TypeVar("_T")

def wrap(f: _T) -> Callable[[object], _T]:
    return lambda _: f

def coinflip() -> bool:
    return True

class MissingOverride(BaseProperty):
    @BaseProperty.prop.setter
    def prop(self, value: int) -> None:  # error: [missing-override-decorator]
        pass

class InvalidExplicitOverride:
    @BaseProperty.prop.setter
    @override
    def prop(self, value: int) -> None:  # error: [invalid-explicit-override]
        pass

class WrappedMethod(BaseProperty):
    @wrap(BaseProperty.method)
    def method(self) -> int:  # error: [missing-override-decorator]
        return 2

class WrappedInvalidExplicitOverride:
    @wrap(BaseProperty.method)
    @override
    def method(self) -> int:  # error: [invalid-explicit-override]
        return 2

class WrappedMethodWithOverrideBranch(BaseProperty):
    if coinflip():
        @wrap(BaseProperty.method)
        def method(self) -> int:  # error: [missing-override-decorator]
            return 2

    else:
        @override
        def method(self) -> int:
            return 3

class WrappedInvalidExplicitOverrideWithUndecoratedBranch:
    if coinflip():
        @wrap(BaseProperty.method)
        @override
        def method(self) -> int:  # error: [invalid-explicit-override]
            return 2

    else:
        def method(self) -> int:
            return 3
```

`stub.pyi`:

```pyi
from abc import ABC, abstractmethod
from typing_extensions import Protocol, overload, override

class StubParent:
    @overload
    def method(self, value: int) -> int: ...
    @overload
    def method(self, value: str) -> str: ...

class StubChild(StubParent):
    @overload
    def method(self, value: int) -> int: ...  # error: [missing-override-decorator]
    @overload
    def method(self, value: str) -> str: ...

class ExplicitStubChild(StubParent):
    @overload
    @override
    def method(self, value: int) -> int: ...
    @overload
    def method(self, value: str) -> str: ...

class OverrideOnSecondOverload(StubParent):
    @overload
    def method(self, value: int) -> int: ...  # error: [missing-override-decorator]
    @overload
    @override
    def method(self, value: str) -> str: ...  # error: [invalid-overload]

class OverrideOnFirstOverload(StubParent):
    @overload
    @override
    def method(self, value: int) -> int: ...
    @overload
    def method(self, value: str) -> str: ...

class StubProtocolInterface(Protocol):
    def method(self) -> int: ...

class StubProtocolImplementation(StubProtocolInterface):
    def method(self) -> int: ...  # error: [missing-override-decorator]

class StubAbstractInterface(ABC):
    @abstractmethod
    def method(self) -> int: ...

class StubAbstractImplementation(StubAbstractInterface):
    def method(self) -> int: ...  # error: [missing-override-decorator]
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
        def method2(self, x: str) -> str: ...  # error: [invalid-explicit-override]
        @overload
        def method2(self, x: int) -> int: ...

    else:
        @override
        def method2(self, x): ...
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
though we may also emit `invalid-overload` on these methods.

```py
from typing_extensions import Any, Callable, override, overload

def lossy_decorator(fn: Callable[..., Any]) -> Callable[..., Any]:
    return fn

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
    # error: [invalid-overload] "`@override` decorator should be applied only to the overload implementation"
    def bar(self, x: str) -> str: ...
    @overload
    @override
    # error: [invalid-overload] "`@override` decorator should be applied only to the overload implementation"
    def bar(self, x: int) -> int: ...
    @override
    # error: [invalid-explicit-override]
    def bar(self, x: str | int) -> str | int:
        return x

    @overload
    @override
    # error: [invalid-overload] "`@override` decorator should be applied only to the overload implementation"
    def baz(self, x: str) -> str: ...
    @overload
    def baz(self, x: int) -> int: ...
    # error: [invalid-explicit-override]
    def baz(self, x: str | int) -> str | int:
        return x

    @overload
    @override
    # error: [invalid-overload] "`@override` decorator should be applied only to the overload implementation"
    def quux(self, x: str) -> str: ...
    @overload
    def quux(self, x: int) -> int: ...
    @lossy_decorator
    # error: [invalid-explicit-override]
    def quux(self, x: str | int) -> str | int:
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

class Child(ParentDataclass):  # error: [subclass-of-dataclass-with-order]
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

    # error: [invalid-typed-dict-statement] "TypedDict class cannot have methods"
    @override
    def copy(self) -> Self: ...

class Grandparent(Any): ...

class Parent(Grandparent, NamedTuple):  # error: [invalid-named-tuple]
    x: int

class Child(Parent):
    @override
    def foo(self): ...  # fine because `Any` is in the MRO
```

## Overloaded methods with explicit receiver annotations

When checking an override, overloads with explicit receiver annotations only need to be considered
if the receiver can be an instance of the subclass. For example, `Child` cannot also be an instance
of the unrelated `@final` class `Restricted`, so the `Restricted`-specific overload does not
constrain `Child.method`.

```toml
[environment]
python-version = "3.13"
```

```py
from __future__ import annotations

from collections.abc import Iterable, Iterator, MutableMapping
from typing import Protocol, TypeVar, final, overload

class Base:
    @overload
    def method(self: Restricted, extra: str) -> None: ...
    @overload
    def method(self) -> None: ...
    def method(self, extra: str = "") -> None: ...

@final
class Restricted(Base): ...

class Child(Base):
    def method(self) -> None: ...

# Regression test for https://github.com/astral-sh/ty/issues/2612: the
# `LiteralString`-specific overload of `str.__iter__` does not constrain a
# method override on a user-defined `str` subclass.
class MyStr(str):
    def __iter__(self) -> Iterator[str]:
        raise NotImplementedError

# Regression test for https://github.com/astral-sh/ty/issues/2693: the
# receiver-specific overloads of `MutableMapping.update` that use protocols
# should not cause a false-positive Liskov violation.
KT = TypeVar("KT")
VT = TypeVar("VT")
VT_co = TypeVar("VT_co", covariant=True)

class Maplike(Protocol[KT, VT_co]):
    def keys(self) -> Iterable[KT]: ...
    def __getitem__(self, key: KT, /) -> VT_co: ...

MapOrItems = Maplike[KT, VT] | Iterable[tuple[KT, VT]]

class MyMapping(MutableMapping[KT, VT]):
    def __getitem__(self, key: KT) -> VT:
        raise NotImplementedError
    def __setitem__(self, key: KT, value: VT) -> None: ...
    def __delitem__(self, key: KT) -> None: ...
    def __iter__(self) -> Iterator[KT]:
        raise NotImplementedError
    def __len__(self) -> int:
        raise NotImplementedError
    def update(self, arg: MapOrItems[KT, VT] = (), /, **kw: VT) -> None: ...

# TODO: We should emit an `invalid-method-override` diagnostic on
# `DeferredChild1.method`. The `DeferredChild1`-specific overload applies to
# this subclass, so its override cannot remove the `extra` parameter.
class DeferredBase:
    @overload
    def method(self) -> None: ...
    @overload
    def method(self: DeferredChild1, extra: str) -> None: ...
    def method(self, extra: str = "") -> None: ...

class DeferredChild1(DeferredBase):
    def method(self) -> None: ...

# TODO: A strict Liskov check would emit an `invalid-method-override`
# diagnostic here too. A subclass could inherit from both `DeferredChild1`
# and `DeferredChild2`, making the receiver-specific overload applicable.
class DeferredChild2(DeferredBase):
    def method(self) -> None: ...
```
