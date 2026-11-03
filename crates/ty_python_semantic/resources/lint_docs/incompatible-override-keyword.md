## What it does

Checks for method overrides that still accept an argument by position but lose the keyword name
accepted by the superclass. This can happen when an override renames a positional-or-keyword
parameter or makes it positional-only. If the override also requires an extra argument or has
another incompatibility, ty reports `incompatible-method-override`.

If one overload still accepts `value=` and another accepts that argument only by position, ty can
report `incompatible-method-override` even when all positional calls still work.

This rule is disabled by default. To report these overrides as errors, set
`incompatible-override-keyword = "error"` in the `[rules]` section of `ty.toml`.

## Why is this bad?

A caller using the superclass's keyword name can get a `TypeError` when given an instance of the
subclass.

## Example

```python
class Base:
    def method(self, value: int) -> None: ...


class Child(Base):
    def method(self, item: int) -> None: ...  # error: [incompatible-override-keyword]


def call_method(obj: Base) -> None:
    obj.method(value=1)


call_method(Child())
```

Keep the name `value` in the override, or make the superclass parameter positional-only if callers
should always pass it by position.
