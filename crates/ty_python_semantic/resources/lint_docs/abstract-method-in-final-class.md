## What it does

Checks for `@final` classes that have unimplemented abstract methods.

## Why is this bad?

A class decorated with `@final` cannot be subclassed. If such a class has abstract
methods that are not implemented, the class can never be properly instantiated, as
the abstract methods can never be implemented (since subclassing is prohibited).

At runtime, instantiation of classes with unimplemented abstract methods is only
prevented for classes that have `ABCMeta` (or a subclass of it) as their metaclass.
However, type checkers also enforce this for classes that do not use `ABCMeta`, since
the intent for the class to be abstract is clear from the use of `@abstractmethod`.

## Example

```python
from abc import ABC, abstractmethod
from typing import final


class Base(ABC):
    @abstractmethod
    def method(self) -> int: ...


@final
# `Derived` does not implement `method`
class Derived(Base):  # error
    pass
```
