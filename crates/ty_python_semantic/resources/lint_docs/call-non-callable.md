## What it does

Checks for calls to non-callable objects.

This includes attempts to instantiate classes with unimplemented abstract methods, whether those
methods are explicitly decorated with `abstractmethod` or implicitly abstract protocol members.

## Why is this bad?

Calling a non-callable object will raise a `TypeError` at runtime.

Abstract methods describe behavior that a subclass must implement before it can be instantiated.
Classes using `ABCMeta` enforce this at runtime; the type checker also enforces it for classes
without `ABCMeta`.

## Examples

```python
# TypeError: 'int' object is not callable
4()  # error
```

```python
from abc import ABC, abstractmethod


class Abstract(ABC):
    @abstractmethod
    def method(self) -> int: ...


# `method` has not been implemented.
Abstract()  # error


class Concrete(Abstract):
    def method(self) -> int:
        return 42


Concrete()  # OK
```
