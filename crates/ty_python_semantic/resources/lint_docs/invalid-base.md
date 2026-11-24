## What it does

Checks for class definitions that have bases which are not instances of `type`.

## Why is this bad?

Class definitions with bases like this will lead to `TypeError` being raised at runtime.

## Examples

```python
class A(42): ...  # error: [invalid-base]
```
