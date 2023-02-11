# bad-quotes-inline-string (Q000)

Derived from the **flake8-quotes** linter.

Autofix is always available.

## What it does
Checks for inline strings that use single quotes or double quotes,
depending on the value of the [`inline-quotes`](https://github.com/charliermarsh/ruff#inline-quotes)
setting.

## Why is this bad?
Consistency is good. Use either single or double quotes for inline
strings, but be consistent.

## Example
```python
foo = 'bar'
```

Assuming `inline-quotes` is set to `double`, use instead:
```python
foo = "bar"
```