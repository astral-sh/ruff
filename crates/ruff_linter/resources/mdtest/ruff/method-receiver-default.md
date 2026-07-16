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
  |
help: Remove default value from receiver parameter
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
    def method(self): ...


class CustomDecoratorReceiverNoDefault:
    @decorator
    def method(self): ...
```
