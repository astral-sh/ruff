## What it does

Checks for default values that can't be
assigned to the parameter's annotated type.

## Why is this bad?

This breaks the rules of the type system and
weakens a type checker's ability to accurately reason about your code.

## Examples

```python
def f(a: int = ""): ...  # error
```
