# yield-in-init (PLE0100)

Derived from the **Pylint** linter.

### What it does
Checks for `__init__` methods that are turned into generators by the
inclusion of `yield` or `yield from` statements.

### Why is this bad?
The `__init__` method of a class is used to initialize new objects, not
create them. As such, it should not return any value. By including a
yield expression in the method turns it into a generator method. On
calling, it will return a generator resulting in a runtime error.

### Example
```python
class InitIsGenerator:
    def __init__(self, i):
        yield i
```

### References
* [`py-init-method-is-generator`](https://codeql.github.com/codeql-query-help/python/py-init-method-is-generator/)