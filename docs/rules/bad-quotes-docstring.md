# bad-quotes-docstring (Q002)

Derived from the **flake8-quotes** linter.

Autofix is always available.

## What it does
Checks for docstrings that use single quotes or double quotes, depending on the value of the [`docstring-quotes`](https://github.com/charliermarsh/ruff#docstring-quotes)
setting.

## Why is this bad?
Consistency is good. Use either single or double quotes for docstring
strings, but be consistent.

## Example
```python
'''
bar
'''
```

Assuming `docstring-quotes` is set to `double`, use instead:
```python
"""
bar
"""
```