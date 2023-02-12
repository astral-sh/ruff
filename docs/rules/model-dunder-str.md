# model-dunder-str (DJ008)

Derived from the **flake8-django** linter.

## What it does
Checks that `__str__` method is defined in Django models.

## Why is this bad?
Django models should define `__str__` method to return a string representation
of the model instance, as Django calls this method to display the object in
the Django Admin and elsewhere.

Models without `__str__` method will display a non-meaningful representation
of the object in the Django Admin.

## Example
```python
from django.db import models

class MyModel(models.Model):
   field = models.CharField(max_length=255)
```

Use instead:
```python
from django.db import models

class MyModel(models.Model):
   field = models.CharField(max_length=255)

  def __str__(self):
     return f"{self.field}"
```