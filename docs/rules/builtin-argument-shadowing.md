# builtin-argument-shadowing (A002)

Derived from the **flake8-builtins** linter.

### What it does

Prevents shadowing builtin variables in function arguments.

### Why is this bad?

Shadowing can obfuscate the purpose of a variable in your code.

### Example

```python
def foo(id):
    ...
```

Instead, use a more specific variable name or suffix the variable with an underscore:

```python
def foo(bar_id):
    ...

def foo(id_):
    ...
```