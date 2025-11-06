# Django - Issue #1018: Model Attributes Not Recognized

Tests for GitHub issue #1018 where ty doesn't recognize Django model attributes.

**Phase**: 0 (Documenting current failures)

## Auto-synthesized Primary Key (id field)

Django automatically adds an `id` field to models without an explicit primary key.

```toml
[environment]
python-version = "3.11"
extra-paths = ["/stubs"]
```

`/stubs/django/__init__.py`:

```py
```

`/stubs/django/db/__init__.py`:

```py
```

`/stubs/django/db/models/__init__.py`:

```py
```

`/stubs/django/db/models/__init__.pyi`:

```pyi
from typing import Any, Generic, TypeVar

_T = TypeVar("_T")

class Model:
    # Django auto-synthesizes these attributes via metaclass
    # TODO: [Phase 2] Auto-add id field when not explicitly defined
    pk: Any
    def save(self, **kwargs: Any) -> None: ...
    def delete(self) -> tuple[int, dict[str, int]]: ...

class Field(Generic[_T]):
    def __get__(self, instance: Any, owner: Any) -> _T: ...
    def __set__(self, instance: Any, value: _T) -> None: ...

class CharField(Field[str]):
    def __init__(self, max_length: int = 255, **kwargs: Any): ...

class Manager(Generic[_T]):
    def all(self) -> Any: ...
    def get(self, **kwargs: Any) -> _T: ...
```

`test.py`:

```py
from django.db import models

class Demo(models.Model):
    demo_name = models.CharField(max_length=100)

# Test case from issue #1018
def test_model_fields(m: Demo):
    # Auto-synthesized id field - produces error since not auto-synthesized
    m.id  # error: [unresolved-attribute] "Object of type `Demo` has no attribute `id`"
    # 1. Current ty (no changes): error[unresolved-attribute] (id field not auto-synthesized)
    # 2. Future ty (after fix): int (no error)
    # 3. mypy + django-stubs: int (no error)
    # 4. pyright + django-types: int (no error)
    # TODO: [Phase 2] Auto-synthesize id field on models without explicit pk

    # Custom CharField field
    reveal_type(m.demo_name)  # revealed: Unknown | str
    # 1. Current ty (no changes): Unknown | str (descriptor protocol partially works!)
    # 2. Future ty (after fix): str
    # 3. mypy + django-stubs: str
    # 4. pyright + django-types: str
    # TODO: [Phase 3] Remove Unknown from union - descriptor works but needs type narrowing

    # Inherited save method
    reveal_type(m.save)  # revealed: bound method Demo.save(**kwargs: Any) -> None
    # 1. Current ty (no changes): bound method Demo.save(**kwargs: Any) -> None
    # 2. Future ty (after fix): bound method Demo.save(**kwargs: Any) -> None (no change needed)
    # 3. mypy + django-stubs: def (**kwargs: Any) -> None
    # 4. pyright + django-types: (**kwargs: Any) -> None

# Class-level .objects manager access - produces error since not auto-synthesized
Demo.objects  # error: [unresolved-attribute] "Class `Demo` has no attribute `objects`"
# 1. Current ty (no changes): error[unresolved-attribute] (.objects not auto-synthesized)
# 2. Future ty (after fix): Manager[Demo] (no error)
# 3. mypy + django-stubs: Manager[Demo] (requires manual annotation)
# 4. pyright + django-types: BaseManager[Demo] (requires manual annotation)
# TODO: [Phase 2] Auto-synthesize .objects manager

# Accessing id via pk alias (should work since pk is in stub)
demo = Demo()
reveal_type(demo.pk)  # revealed: Any
# 1. Current ty (no changes): Any (pk is defined in stub)
# 2. Future ty (after fix): Any (no change needed - pk defined in base Model)
# 3. mypy + django-stubs: Any
# 4. pyright + django-types: Any
```

## Related Issue

This test directly addresses GitHub issue #1018 where users reported:
- `m.id` not recognized
- `m.demo_name` not recognized
- `Demo.objects` not recognized
- `m.save` works (inherited from Model base class)

The root cause is Django's metaclass-based attribute synthesis, which requires
dedicated Django support in the type checker.
