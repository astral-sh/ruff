## What it does

Checks for direct instantiation of protocol classes and classes with unimplemented abstract methods.

## Why is this bad?

Abstract methods describe behavior that a subclass must implement before it can be instantiated.
Classes using `ABCMeta` enforce this at runtime. ty also checks classes without `ABCMeta` and
rejects direct instantiation of protocols.

## Examples

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

```python
from typing import Protocol


class HasName(Protocol):
    name: str


HasName()  # error
```
