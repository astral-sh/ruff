# f-string-missing-placeholders (F541)

Derived from the **Pyflakes** linter.

Autofix is always available.

## What it does
Checks for f-strings that do not contain any placeholder expressions.

## Why is this bad?
F-strings are a convenient way to format strings, but they are not
necessary if there are no placeholder expressions to format. In this
case, a regular string should be used instead, as an f-string without
placeholders can be confusing for readers, who may expect such a
placeholder to be present.

An f-string without any placeholders could also indicate that the
author forgot to add a placeholder expression.

## Example
```python
f"Hello, world!"
```

Use instead:
```python
"Hello, world!"
```

## References
* [PEP 498](https://www.python.org/dev/peps/pep-0498/)