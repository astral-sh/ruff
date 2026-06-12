## What it does

Checks for invalid match patterns.

## Why is this bad?

Matching on invalid patterns will lead to a runtime error.

## Examples

```python
NotAClass = 42

match object():
    # TypeError at runtime: must be a class
    case NotAClass():  # error
        ...
```
