## What it does

Detects method overrides that rename a positional-or-keyword parameter or make it positional-only.

## Why is this bad?

Code that uses the base class's interface can pass the argument by keyword. The same call may raise
`TypeError` on an instance of the subclass.

## Example

```python
class Base:
    def scale(self, factor: int) -> int:
        return factor * 2


class Child1(Base):
    def scale(self, multiplier: int) -> int:  # error: [missing-keyword-in-override]
        return multiplier * 3


class Child2(Base):
    def scale(self, factor: int, /) -> int:  # error: [missing-keyword-in-override]
        return factor * 3


def scale_by_two(obj: Base) -> int:
    return obj.scale(factor=2)


scale_by_two(Child1())  # Raises TypeError, as would passing Child2().
```

Keep `factor` as a positional-or-keyword parameter in the override so both calling conventions work.

## See also

- `incompatible-method-override` reports other signature incompatibilities, such as a narrower
    parameter type or an incompatible return type, even when the override also drops a keyword name.
