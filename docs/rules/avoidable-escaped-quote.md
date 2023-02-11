# avoidable-escaped-quote (Q003)

Derived from the **flake8-quotes** linter.

Autofix is always available.

## What it does
Checks for strings that include escaped quotes, and suggests changing
the quote style to avoid the need to escape them.

## Why is this bad?
It's preferable to avoid escaped quotes in strings. By changing the
outer quote style, you can avoid escaping inner quotes.

## Example
```python
foo = 'bar\'s'
```

Use instead:
```python
foo = "bar's"
```