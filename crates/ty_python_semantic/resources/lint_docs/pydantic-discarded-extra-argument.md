## What it does

Checks for extra keyword arguments that Pydantic silently discards when a model uses
`extra="ignore"`, either implicitly or explicitly.

## Why is this bad?

A discarded argument has no effect on the constructed model, but it may indicate a misspelled field
name or an incorrect assumption about the model's schema.

## Example

```python {data-mdtest="ignore"}
from pydantic import BaseModel


class User(BaseModel):
    name: str
    admin: bool = False


user = User(name="Alice", admni=True)  # error: [pydantic-discarded-extra-argument]
```

If the field name has been misspelled, fix the typo. Otherwise, consider removing the extra argument,
or explicitly configure the model with `extra="allow"`.
