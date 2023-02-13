# model-string-field-nullable (DJ001)

Derived from the **flake8-django** linter.

## What it does
Checks nullable string-based fields (like `CharField` and `TextField`)
in Django models.

## Why is this bad?
If a string-based field is nullable, then your model will have two possible
representations for "no data": `None` and the empty string. This can lead to
confusion, as clients of the API have to check for both `None` and the
empty string when trying to determine if the field has data.

The Django convention is to use the empty string in lieu of `None` for
string-based fields.

## Example
```python
from django.db import models

class MyModel(models.Model):
   field = models.CharField(max_length=255, null=True)
```

Use instead:
```python
from django.db import models

class MyModel(models.Model):
    field = models.CharField(max_length=255, default="")
```