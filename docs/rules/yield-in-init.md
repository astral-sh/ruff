# yield-in-init (PLE0100)

Derived from the **Pylint** linter.

## What it does
Checks for `__init__` methods that are turned into generators by the
inclusion of `yield` or `yield from` expressions.

## Why is this bad?
The `__init__` method is the constructor for a given Python class,
responsible for initializing, rather than creating, new objects.

The `__init__` method has to return `None`. By including a `yield` or
`yield from` expression in an `__init__`, the method will return a
generator object when called at runtime, resulting in a runtime error.

## Example
```python
class InitIsGenerator:
    def __init__(self, i):
        yield i
```

## References
* [`py-init-method-is-generator`](https://codeql.github.com/codeql-query-help/python/py-init-method-is-generator/)