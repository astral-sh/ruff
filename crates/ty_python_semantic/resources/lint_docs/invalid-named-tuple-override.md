## What it does

Checks for subclass members that override inherited `NamedTuple` fields.

## Why is this bad?

Reusing an inherited `NamedTuple` field name in a subclass creates a
class where tuple indexing and `repr()` still reflect the original
field, while attribute access follows the subclass member.

## Default level

This rule is a warning by default because these overrides do not make
the class invalid at runtime.

## Examples

```python
from typing import NamedTuple


class User(NamedTuple):
    name: str


class Admin(User):
    name = "shadowed"  # error: [invalid-named-tuple-override]


admin = Admin("Alice")
admin.name  # "shadowed"
admin[0]  # "Alice"
```
