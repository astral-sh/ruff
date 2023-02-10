# yield-in-init (PLE0100)

Derived from the **Pylint** linter.

### What it does
Checks for `__init__` methods that turned into generators
via the presence of `yield` or `yield from` statements.

### Why is this bad?
Generators are not allowed in `__init__` methods.

### Example
```python
class Foo:
    def __init__(self):
        yield 1
```