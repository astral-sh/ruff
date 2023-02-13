# builtin-attribute-shadowing (A002)

Derived from the **flake8-builtins** linter.

### What it does

Prevents attributes and methods from having the same name as a builtin variable.

### Why is this bad?

Shadowing can obfuscate the purpose of a variable in your code.

### Example
```python
class Foo:
    type: int

    def print(self):
        ...
```

Instead, use a more specific variable name or suffix the variable with an underscore:

```python
class Foo:
    type_: int
    food_type: int

    def print_(self):
        ...
```
