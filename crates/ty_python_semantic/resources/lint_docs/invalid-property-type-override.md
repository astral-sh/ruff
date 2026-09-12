## What it does

Detects overrides that change the readable type of an inherited property incompatibly, restrict the
values accepted by its setter, or remove a required write operation.

## Why is this bad?

Code using the superclass interface must continue to work on subclass instances. A getter can return
a more specific type, but a setter must accept every value accepted by the superclass. A writable
attribute cannot be replaced by a read-only property.

## Example

```python
class Base:
    @property
    def value(self) -> int:
        return 0


class Child(Base):
    @property
    def value(self) -> str:  # error: [invalid-property-type-override]
        return ""
```

An override returning `bool` would be valid because `bool` is a subtype of `int`.
