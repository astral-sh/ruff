# Django Type Checker Comparison

This document compares how different Python type checkers handle Django ORM code.

## Test Code

```python
from django.db import models
from django.db.models.manager import Manager


class User(models.Model):
    name = models.CharField(max_length=100)
    email = models.EmailField()
    age = models.IntegerField()

    objects = Manager["User"]()  # Required for type checking


# Test cases
reveal_type(User)                                    # Test 1: Model class type
reveal_type(User.objects)                            # Test 2: Manager type
all_users = User.objects.all()
reveal_type(all_users)                               # Test 3: QuerySet type
user = User.objects.get(id=1)
reveal_type(user)                                    # Test 4: Model instance from get()
reveal_type(user.name)                               # Test 5: CharField access
reveal_type(user.email)                              # Test 6: EmailField access
reveal_type(user.age)                                # Test 7: IntegerField access
reveal_type(user.nonexistent)                        # Test 8: Invalid attribute
User.objects.nonexistent_method()                    # Test 9: Invalid method
filtered = User.objects.filter(age=25).exclude(name="test")
reveal_type(filtered)                                # Test 10: QuerySet chaining
```

## Results Comparison

| Test | Mypy + django-stubs | Pyright + django-types | ty (Current) |
|------|---------------------|------------------------|--------------|
| **1. Model class type** | `def (*args: Any, **kwargs: Any) -> User` | `type[User]` | `<class 'User'>` ✅ |
| **2. Manager type** | `Manager[User]` | `BaseManager[User]` | Import error |
| **3. QuerySet type** | `QuerySet[User, User]` | `BaseManager[User]` | Import error |
| **4. Model instance** | `User` | `User` | Import error |
| **5. CharField access** | `str` ✅ | `str` ✅ | Import error |
| **6. EmailField access** | `str` ✅ | `str` ✅ | Import error |
| **7. IntegerField access** | `int` ✅ | `int` ✅ | Import error |
| **8. Invalid attribute** | Error: "User" has no attribute | Error: Cannot access attribute | Import error |
| **9. Invalid method** | Error: "Manager[User]" has no attribute | Error: Cannot access attribute | Import error |
| **10. QuerySet chaining** | `QuerySet[User, User]` | `BaseManager[User]` | Import error |

## Key Findings

### Mypy + django-stubs

**Configuration Required:**
```ini
[mypy]
plugins = mypy_django_plugin.main

[mypy.plugins.django-stubs]
django_settings_module = test_settings
```

**Key Features:**
- ✅ Full field type inference (CharField → str, IntegerField → int)
- ✅ Generic `Manager[T]` and `QuerySet[T, T]` (double generic)
- ✅ Method chaining preserves QuerySet type
- ⚠️ Requires explicit `objects = Manager["User"]()` annotation
- ⚠️ Shows "ambiguous" warnings when accessing `.objects` via class

**Verdict:** Most comprehensive Django support via mypy plugin

### Pyright + django-types

**Requirements:**
```bash
pip install django-types
```

**Key Features:**
- ✅ Full field type inference (CharField → str, IntegerField → int)
- ✅ `BaseManager[T]` type for managers
- ✅ ForeignKey types resolved correctly (`post.author` → `User`)
- ⚠️ Requires explicit `objects = Manager["User"]()` annotation
- ⚠️ `.all()` and `.filter()` return `BaseManager[T]` instead of `QuerySet[T]`

**Verdict:** Good Django support with simpler type model (BaseManager instead of QuerySet)

### ty (Current State)

**Current Behavior:**
- ❌ Cannot resolve Django imports without stubs
- ✅ Basic Model class detection works with stubs
- ❌ No `.objects` auto-synthesis
- ❌ No field type inference
- ❌ No QuerySet/Manager generic support

**With Our Test Stubs:**
- ✅ Model class type detection
- ✅ Basic methods (save, delete) accessible
- ⏳ Generic support requires manual annotations
- ⏳ Field access not type-safe (returns Any)

## Implementation Gap Analysis

To match mypy + django-stubs, ty needs:

### Phase 2 (Critical):
1. **Auto-synthesize `.objects` manager** on Model classes
   - Both mypy and pyright require manual `objects = Manager["User"]()` annotation
   - **ty could do better** by auto-synthesizing this!
2. **Auto-synthesize exception classes** (`.DoesNotExist`, `.MultipleObjectsReturned`)
3. **Auto-add `id` field** when not explicitly defined

### Phase 3 (Field Descriptors):
4. **Implement descriptor protocol** for Django Fields
   - `CharField` should make attribute access return `str`
   - `IntegerField` → `int`, `BooleanField` → `bool`, etc.
   - This is what makes `user.name` return `str` instead of `Any`

### Phase 4 (Advanced):
5. **Generic Manager/QuerySet support**
   - `Manager[User]` and `QuerySet[User]` types
   - Method chaining type preservation

## Competitive Advantage Opportunity

**ty could provide BETTER Django support than mypy/pyright by:**

1. **Auto-synthesizing `.objects`** - users wouldn't need to manually add `objects = Manager["User"]()`
2. **Smarter field inference** - detecting field types without needing descriptor implementation
3. **Better error messages** - Django-specific error messages for common mistakes

## Test File Location

The comparison test file is at: `/Users/saada/hack/ty/ruff/django_comparison_test.py`

## Next Steps

1. Implement Phase 2 (auto-synthesis) to match baseline mypy/pyright behavior
2. Implement Phase 3 (field descriptors) to match field type inference
3. Consider auto-synthesis improvements over mypy/pyright's manual annotation requirement
