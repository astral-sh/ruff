# builtin-variable-shadowing (A001)

Derived from the **flake8-builtins** linter.

### What it does

Prevents shadowing builtin variables with user defined variables.

### Why is this bad?

Shadowing can make your code harder to understand. Shadowing will also cause type errors
if you use a shadowed variable as a type.

### Example

```python
def foo():
    id = 1
```

Instead, use a more specific variable name or suffix with an underscore:

```python
def foo():
    bar_id = 1

def foo():
    id_ = 1
```
