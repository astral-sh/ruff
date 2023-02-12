# unnecessary-map (C417)

Derived from the **flake8-comprehensions** linter.

Autofix is sometimes available.

## What it does
Checks for unnecessary `map` calls with `lambda` functions.

## Why is this bad?
Using `map(func, iterable)` when `func` is a `lambda` is slower than
using a generator expression or a comprehension, as the latter approach
avoids the function call overhead, in addition to being more readable.

## Examples
```python
map(lambda x: x + 1, iterable)
```

Use instead:
```python
(x + 1 for x in iterable)
```

This rule also applies to `map` calls within `list`, `set`, and `dict`
calls. For example:
* Instead of `list(map(lambda num: num * 2, nums))`, use
  `[num * 2 for num in nums]`.
* Instead of `set(map(lambda num: num % 2 == 0, nums))`, use
  `{num % 2 == 0 for num in nums}`.
* Instead of `dict(map(lambda v: (v, v ** 2), values))`, use
  `{v: v ** 2 for v in values}`.