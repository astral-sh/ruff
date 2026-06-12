## What it does
Detects attribute overrides that change whether an inherited attribute
is a class variable or an instance variable.

This rule currently only covers class-variable and instance-variable
category changes.

## Why is this bad?
Pure class variables and instance variables have different access and
assignment behavior. Overriding one with the other violates the
[Liskov Substitution Principle] ("LSP"), because code that is valid for
the superclass may no longer be valid for the subclass.

## Example
```python
from typing import ClassVar

class Base:
    instance_attr: int
    class_attr: ClassVar[int]

class Sub(Base):
    instance_attr: ClassVar[int]  # error: [invalid-attribute-override]
    class_attr: int  # error: [invalid-attribute-override]
```

[Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle
