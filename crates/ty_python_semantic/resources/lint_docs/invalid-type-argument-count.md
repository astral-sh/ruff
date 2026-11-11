## What it does

Checks the number of arguments explicitly supplied in brackets to generic types and some typing
forms, including `Callable`.

## Why is this bad?

A generic type needs an argument for each required type parameter. Parameters with defaults can be
omitted, and a variadic parameter can accept multiple arguments. Typing forms such as `Callable`
also require a specific number of arguments.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
from typing import Callable


class Box[T]: ...


type Pair[T, U] = tuple[T, U]


a: Box[int, str]  # error
b: Pair[int]  # error
c: Callable[[], int, str]  # error
```

## Rule history

- v0.0.102: Split out from `invalid-type-arguments` and `invalid-type-form`. Use
    `invalid-type-argument-count` in rule settings and ignore comments for the moved checks. Keep
    the old selectors wherever you still want to control the checks they cover.
