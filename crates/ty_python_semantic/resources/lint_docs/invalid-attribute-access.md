## What it does

Checks for assignments to class variables from instances
and assignments to instance variables from its class.

## Why is this bad?

Incorrect assignments break the rules of the type system and
weaken a type checker's ability to accurately reason about your code.

## Examples

```python
from typing import ClassVar


class C:
    class_var: ClassVar[int] = 1
    instance_var: int


C.class_var = 3  # okay
# Cannot assign to class variable
C().class_var = 3  # error
```
