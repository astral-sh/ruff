## What it does

Detects method overrides that drop the keyword name of a positional-or-keyword parameter from a base
method while still accepting the argument by position. Renaming a parameter, making it
positional-only, or absorbing it into `*args` can drop its keyword name.

When an override also violates the base method's signature in another way, such as narrowing a
parameter's type or returning an incompatible type, ty reports `incompatible-method-override`
instead.

## Why is this bad?

Code that uses the base class's interface can pass the argument by keyword. The same call may raise
`TypeError` on an instance of the subclass.

## Example

```python
class Base:
    def scale(self, factor: int) -> int:
        return factor * 2


class Child(Base):
    def scale(self, multiplier: int) -> int:  # error: [missing-keyword-in-override]
        return multiplier * 3


def scale_by_two(obj: Base) -> int:
    return obj.scale(factor=2)


scale_by_two(Child())  # Raises TypeError.
```

Preserve the parameter name `factor` in the override so that both calling conventions work.
