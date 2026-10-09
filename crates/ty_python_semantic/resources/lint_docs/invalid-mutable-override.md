## What it does

Detects overrides that narrow the type of a writable attribute.

This rule is disabled by default. Enable it to enforce invariance of mutable attributes.

## Why is this bad?

A subclass must accept every write allowed by the superclass. Narrowing a mutable attribute's type
allows code using the superclass interface to invalidate the subclass's attribute type.

## Example

```python
class Base:
    value: int


class Child(Base):
    value: bool  # error: [invalid-mutable-override]


def reset(obj: Base) -> None:
    obj.value = 42


reset(Child())  # The value is no longer a bool.
```

Use the same annotation in both classes, or expose a read-only property if callers do not need to
write the attribute.
