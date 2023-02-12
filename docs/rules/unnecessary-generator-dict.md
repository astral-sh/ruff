# unnecessary-generator-dict (C402)

Derived from the **flake8-comprehensions** linter.

Autofix is always available.

## What it does
Checks for unnecessary generator that can be rewritten as `dict` comprehension.

## Why is this bad?
It is unnecessary to use `dict` around a generator expression, since there are
equivalent comprehensions for these types.

## Examples
```python
dict((x, f(x)) for x in foo)
```

Use instead:
```python
{x: f(x) for x in foo}
```