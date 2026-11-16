## What it does

Checks for `TypeIs[T]` return annotations where `T` is disjoint from and [not assignable to] the
declared type of the parameter being narrowed.

## Why is this bad?

When a `TypeIs[T]` function returns `True`, ty narrows the argument to the values that match both
its existing type and `T`. If no value can belong to both types simultaneously, the check can never
truthfully return `True` for an argument of the declared parameter type.

## Examples

```python
from typing_extensions import TypeIs


def is_string(value: int) -> TypeIs[str]:  # error
    return isinstance(value, str)
```

If the function is meant to distinguish strings from integers, accept both in the parameter type:

```python
def is_string(value: int | str) -> TypeIs[str]:
    return isinstance(value, str)
```

## See also

This rule exclusively covers cases where the parameter type is disjoint from the return type, which
is always unsound. It sibling rule, `suspicious-type-is-narrowing`, covers cases where the two
types are not disjoint, but where the return annotation is not a subtype of the parameter
annotation.

## References

[Typing specification] rules for `TypeIs` functions.

## Rule history

- v0.0.1-alpha.11: First reported as part of `invalid-type-guard-definition`.
- v0.0.105: Split out from `invalid-type-guard-definition` as a standalone rule.

[not assignable to]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable
[typing specification]: https://typing.python.org/en/latest/spec/narrowing.html#typeis
