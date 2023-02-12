# unnecessary-generator-set (C401)

Derived from the **flake8-comprehensions** linter.

Autofix is always available.

## What it does
Checks for unnecessary generator that can be rewritten as `set` comprehension.

## Why is this bad?
It is unnecessary to use `set` around a generator expression, since there are
equivalent comprehensions for these types.

## Examples
```python
set(f(x) for x in foo)
```

Use instead:
```python
{f(x) for x in foo}
```