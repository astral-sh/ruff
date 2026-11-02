## What it does

Detects attribute overrides that change whether an inherited attribute is reachable through the
class object or only through an instance, expose an incompatible value type, or make a writable
attribute read-only.

Narrowing a mutable attribute's type is checked separately by the `invalid-mutable-override` rule.
Overrides involving properties are checked by `invalid-property-type-override`.

## Why is this bad?

Pure class variables and instance-only attributes have different access and assignment behavior.
Overriding one with the other violates the
[Liskov Substitution Principle][liskov-substitution-principle] ("LSP"), because code that is valid
for the superclass may no longer be valid for the subclass.

An attribute declared in the class body without `ClassVar` is reachable both on the class object and
on instances. Overriding a `ClassVar` with one of those withholds nothing the superclass promised,
so that case is allowed.

## Example

```python
from typing import ClassVar, Final


class Base:
    instance_attr: int
    class_attr: ClassVar[int]
    name: str
    count: int


class Sub(Base):
    # A `ClassVar` cannot be assigned through an instance, so it cannot stand in
    # for an attribute that the superclass let instances assign.
    instance_attr: ClassVar[int]  # error: [invalid-attribute-override]

    # A regular class-body attribute is reachable through both the class object
    # and an instance, so overriding a `ClassVar` with one is fine.
    class_attr: int

    name: bytes  # error: [invalid-attribute-override]
    count: Final[int] = 0  # error: [invalid-attribute-override]
```

Code that reads `Base.name` on an instance expects a string. Code that accepts a `Base` can also
assign to `count`; making it final in the subclass removes that operation.

[liskov-substitution-principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle
