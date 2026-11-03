## What it does

Checks that the narrowed type `T` in a `TypeIs[T]` return annotation is assignable to the declared
type of the parameter being narrowed.

## Why is this bad?

A `TypeIs` function refines the type of its argument in both branches of a conditional. Its narrowed
type must therefore be consistent with the input type. For example, `list[str]` cannot be assigned
to `list[object]` because code that accepts a `list[object]` could append an integer to it. A
`TypeGuard` can express this narrowing for the positive branch.

## Examples

```toml
[environment]
python-version = "3.13"
```

```python
from typing import TypeGuard, TypeIs


def is_str_list(items: list[object]) -> TypeIs[list[str]]:  # error
    return all(isinstance(item, str) for item in items)


def contains_only_strings(items: list[object]) -> TypeGuard[list[str]]:
    return all(isinstance(item, str) for item in items)
```

Before ty 0.0.97, `invalid-type-guard-definition` reported both missing parameters to narrow and
incompatible `TypeIs` narrowings. To keep a custom severity for both checks, set it on both rules.
For a `ty: ignore[invalid-type-guard-definition]` that suppresses an incompatible `TypeIs`
narrowing, replace the rule name with `invalid-type-is-narrowing`.
