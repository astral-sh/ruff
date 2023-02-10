# builtin-attribute-shadowing (A002)

Derived from the **flake8-builtins** linter.

### What it does

Prevents attributes and methods from having the same name as a builtin variable.

### Why is this bad?

Shadowing can make your code harder to understand. Shadowing will also cause type errors
if you use a shadowed variable as a type.


### Example
```python
class Foo:
    type: int

    def print(self):
        ...
```

Use instead:

```python
class Foo:
    type_: int

    def print_(self):
        ...
```
