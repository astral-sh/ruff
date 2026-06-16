## What it does

Checks for exception handlers that catch non-exception classes.

## Why is this bad?

Catching classes that do not inherit from `BaseException` will raise a `TypeError` at runtime.

## Example

```python
import random


def might_raise() -> float:
    return 1 / random.choice([0, 1, 2, 3, 4, 5])


try:
    might_raise()
except 1:  # error
    ...
```

Use instead:

```python
import random


def might_raise() -> float:
    return 1 / random.choice([0, 1, 2, 3, 4, 5])


try:
    might_raise()
except ZeroDivisionError:
    ...
```

## References

- [Python documentation: except clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
- [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)

## Ruff rule

This rule corresponds to Ruff's [`except-with-non-exception-classes` (`B030`)](https://docs.astral.sh/ruff/rules/except-with-non-exception-classes)
