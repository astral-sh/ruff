## What it does

Detects attribute overrides that remove inherited access or assignment operations, expose an
incompatible value type, or make a writable attribute read-only.

Narrowing a mutable attribute's type is checked separately by the opt-in `invalid-mutable-override`
rule. Overrides involving properties are checked by `invalid-property-type-override`.

Explicit protocol implementations must also preserve `ClassVar` declarations, matching the
requirement for structural protocol implementations.

## Why is this bad?

A subclass must preserve the operations available on inherited attributes. For example, replacing an
attribute writable through instances with a pure class variable violates the
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
    instance_attr: ClassVar[int]  # error: [invalid-attribute-override]
    name: bytes  # error: [invalid-attribute-override]
    count: Final[int] = 0  # error: [invalid-attribute-override]

    def __init__(self) -> None:
        self.class_attr: int = 1  # error: [invalid-attribute-override]
```

Code that reads `Base.name` on an instance expects a string. Code that accepts a `Base` can also
assign to `count`; making it final in the subclass removes that operation.

[liskov-substitution-principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle
