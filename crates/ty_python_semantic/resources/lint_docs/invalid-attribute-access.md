## What it does

Checks for assignments to class variables from instances
and assignments to instance variables from its class.

## Why is this bad?

Incorrect assignments break the rules of the type system and
weaken a type checker's ability to accurately reason about your code.

## Examples

```python
class C:
    class_var: ClassVar[int] = 1
    instance_var: int


C.class_var = 3  # okay
C().class_var = 3  # error: Cannot assign to class variable

C().instance_var = 3  # okay
C.instance_var = 3  # error: Cannot assign to instance variable
```
