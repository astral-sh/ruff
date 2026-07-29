## What it does

Checks for calls to abstract `@classmethod`s or `@staticmethod`s
with "trivial bodies" when accessed on the class object itself.

"Trivial bodies" are bodies that solely consist of `...`, `pass`,
a docstring, and/or `raise NotImplementedError`.

## Why is this bad?

An abstract method with a trivial body has no concrete implementation
to execute, so calling such a method directly on the class will probably
not have the desired effect.

It is also unsound to call these methods directly on the class. Unlike
other methods, ty permits abstract methods with trivial bodies to have
non-`None` return types even though they always return `None` at runtime.
This is because it is expected that these methods will always be
overridden rather than being called directly. As a result of this
exception to the normal rule, ty may infer an incorrect type if one of
these methods is called directly, which may then mean that type errors
elsewhere in your code go undetected by ty.

Calling abstract classmethods or staticmethods via `type[X]` is allowed,
since the actual runtime type could be a concrete subclass with an implementation.

## Example

```python
from abc import ABC, abstractmethod


class Foo(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int: ...


# cannot call abstract classmethod
Foo.method()  # error
```
