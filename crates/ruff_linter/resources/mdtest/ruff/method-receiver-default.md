# `method-receiver-default` (`RUF077`)

```toml
lint.preview = true
lint.select = ["RUF077"]
```

## Basic errors

```py
class InstanceReceiverDefault:
    def method(self=None): ...  # snapshot: method-receiver-default


class ClassReceiverDefault:
    @classmethod
    def build(cls=None): ...  # error: [method-receiver-default]


class NewMethodClassReceiver:
    def __new__(cls=None): ...  # error: [method-receiver-default]


class NestedInIfReceiverDefault:
    if True:
        def method(self=None): ...  # error: [method-receiver-default]
```

```snapshot
error[RUF077]: Instance receiver parameter should not have a default value
 --> src/mdtest_snippet.py:2:21
  |
2 |     def method(self=None): ...  # snapshot: method-receiver-default
  |                     ^^^^
help: Remove default value from receiver parameter
```

## Implicit classmethods

`__init_subclass__` and `__class_getitem__` are implicit classmethods even without an explicit
`@classmethod` decorator.

```py
class InitSubclassReceiver:
    def __init_subclass__(cls=None): ...  # error: [method-receiver-default]


class ClassGetitemReceiver:
    def __class_getitem__(cls=None, item=None): ...  # error: [method-receiver-default]
```

## Metaclasses

On a metaclass, a plain method's first parameter is conventionally `cls`, not `self`, because
instances of a metaclass are themselves classes. The diagnostic should reflect that.

```py
class Meta(type):
    def method(cls=None): ...  # snapshot: method-receiver-default
```

```snapshot
error[RUF077]: Class receiver parameter should not have a default value
 --> src/mdtest_snippet.py:2:20
  |
2 |     def method(cls=None): ...  # snapshot: method-receiver-default
  |                    ^^^^
help: Remove default value from receiver parameter
```

## Nested classes

```py
class Outer:
    class Inner:
        def method(self=None): ...  # error: [method-receiver-default]
```

## No errors

```py
from typing import override


def decorator(func):
    return func


class StaticMethodWithDefault:
    @staticmethod
    def helper(arg=None): ...


class InstanceReceiverNoDefault:
    def method(self): ...


class ClassReceiverNoDefault:
    @classmethod
    def build(cls): ...


class OverrideReceiverNoDefault:
    @override
    def method(self=None): ...


class CustomDecoratorReceiverNoDefault:
    @decorator
    def method(self=None): ...


class StackedDecoratorsClassMethod:
    @decorator
    @classmethod
    def build(cls=None): ...


class StackedDecoratorsNewMethod:
    @decorator
    def __new__(cls=None): ...


class StackedDecoratorsInitSubclass:
    @decorator
    @decorator
    def __init_subclass__(cls=None): ...


class AbstractStaticMethodWithDefault:
    from abc import abstractmethod

    @staticmethod
    @abstractmethod
    def helper(arg=None): ...


class PropertySetterReceiverNoDefault:
    @property
    def value(self): ...

    @value.setter
    def value(self=None, new_value=None): ...


class NoReceiverParams:
    def method(): ...  # no positional parameters at all; nothing to flag


class OverriddenInitSubclassReceiver:
    from typing import override

    @override
    def __init_subclass__(cls=None): ...
```
