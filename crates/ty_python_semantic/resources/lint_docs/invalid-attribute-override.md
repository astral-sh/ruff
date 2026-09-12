## What it does

Detects attribute overrides that change whether an inherited attribute is a class variable or an
instance variable, expose an incompatible value type, or make a writable attribute read-only.

Narrowing a mutable attribute's type is checked separately by the opt-in `invalid-mutable-override`
rule. Overrides involving properties are checked by `invalid-property-type-override`.

## Why is this bad?

Pure class variables and instance variables have different access and assignment behavior.
Overriding one with the other violates the
[Liskov Substitution Principle][liskov-substitution-principle] ("LSP"), because code that is valid
for the superclass may no longer be valid for the subclass.

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

[liskov-substitution-principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle
