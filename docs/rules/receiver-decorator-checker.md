# receiver-decorator-checker (DJ013)

Derived from the **flake8-django** linter.

## What it does
Checks that Django's `@receiver` decorator is listed first, prior to
any other decorators.

## Why is this bad?
Django's `@receiver` decorator is special in that it does not return
a wrapped function. Rather, `@receiver` connects the decorated function
to a signal. If any other decorators are listed before `@receiver`,
the decorated function will not be connected to the signal.

## Example
```python
from django.dispatch import receiver
from django.db.models.signals import post_save

@transaction.atomic
@receiver(post_save, sender=MyModel)
def my_handler(sender, instance, created, **kwargs):
   pass
```

Use instead:
```python
from django.dispatch import receiver
from django.db.models.signals import post_save

@receiver(post_save, sender=MyModel)
@transaction.atomic
def my_handler(sender, instance, created, **kwargs):
    pass
```