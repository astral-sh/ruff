# builtin-variable-shadowing (A001)

Derived from the **flake8-builtins** linter.

### What it does

Prevents shadowing builtin variables with user defined variables.

### Why is this bad?

Shadowing can obfuscate the purpose of a variable in your code.

### Example

```python
def foo():
    id = 1
```

Instead, use a more specific variable name or suffix the variable with an underscore:

```python
def foo():
    bar_id = 1

def foo():
    id_ = 1
```
