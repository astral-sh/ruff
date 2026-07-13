# `method-receiver-default` (`RUF077`)

```toml
lint.preview = true
# lint.select = ["RUF077"]
```

## Basic errors

```py
class InstanceReceiverDefault:
    def method(self=None): ...  # TODO: error: [method-receiver-default]


class ClassReceiverDefault:
    @classmethod
    def build(cls=None): ...  # TODO: error: [method-receiver-default]


class NewMethodClassReceiver:
    def __new__(cls=None): ...  # TODO: error: [method-receiver-default]


class OverrideReceiverDefault:
    @override
    def method(self=None): ...


class CustomDecoratorReceiverDefault:
    @decorator
    def method(self=None): ...
```

## No errors

```py
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
