## What it does

Checks for methods decorated with both `@abstractmethod` and `@final`.

## Why is this bad?

An abstract method must be overridden for a subclass to become concrete, but a final
method cannot be overridden. Combining the decorators therefore makes it impossible
for a subclass to provide a concrete implementation.

## Example

```python
from abc import ABC, abstractmethod
from typing import final


class Base(ABC):
    @final
    @abstractmethod
    def method(self) -> None: ...  # error
```
