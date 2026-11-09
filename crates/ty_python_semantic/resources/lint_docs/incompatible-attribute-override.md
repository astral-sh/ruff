## What it does

Detects attribute overrides that change whether an inherited attribute is a class variable or an
instance variable, expose an incompatible value type, or make a writable attribute read-only.

Narrowing a mutable attribute's type is checked separately by the `incompatible-mutable-override`
rule. Overrides involving properties are checked by `incompatible-property-override`.

## Why is this bad?

Pure class variables and instance variables have different access and assignment behavior.
Overriding one with the other violates the
[Liskov Substitution Principle][liskov-substitution-principle] ("LSP"), because code that is valid
for the superclass may no longer be valid for the subclass.

## Example

```python
from typing import ClassVar, Final


class Base:
    instance_attr: int
    class_attr: ClassVar[int]
    name: str
    count: int


class Sub(Base):
    instance_attr: ClassVar[int]  # error: [incompatible-attribute-override]
    class_attr: int  # error: [incompatible-attribute-override]
    name: bytes  # error: [incompatible-attribute-override]
    count: Final[int] = 0  # error: [incompatible-attribute-override]
```

Code that reads `Base.name` on an instance expects a string. Code that accepts a `Base` can also
assign to `count`; making it final in the subclass removes that operation.

## Rule history

- v0.0.100: Renamed from `invalid-attribute-override`.

    `invalid-attribute-override` remains as a compatibility alias for this rule.

[liskov-substitution-principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle
