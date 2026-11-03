## What it does

Checks for direct instantiation of protocol classes and classes with unimplemented abstract methods.
These methods can be explicitly decorated with `@abstractmethod`. A subclass can also inherit an
abstract method from a protocol that declares the method without implementing it.

## Why is this bad?

Abstract methods describe behavior that a subclass must implement before it can be instantiated.
Classes using `ABCMeta` enforce this at runtime. ty also enforces it for classes without `ABCMeta`.
Protocol classes define interfaces and cannot be instantiated directly, even if they have no
abstract methods.

A parameter annotated with `type[Base]` can refer to a concrete subclass, so ty allows calls through
the parameter even when `Base` is abstract.

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
