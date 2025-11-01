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

| Test | Mypy + django-stubs | Pyright + django-types | ty + django-stubs |
|------|---------------------|------------------------|-------------------|
| **1. Model class type** | `def (*args: Any, **kwargs: Any) -> User` | `type[User]` | `<class 'User'>` ✅ |
| **2. Manager type** | `Manager[User]` | `BaseManager[User]` | `Unknown \| Manager[User]` ⚠️ |
| **3. QuerySet type** | `QuerySet[User, User]` | `BaseManager[User]` | `Unknown \| QuerySet[User, User]` ✅ |
| **4. Model instance** | `User` | `User` | `Unknown \| User` ⚠️ |
| **5. CharField access** | `str` ✅ | `str` ✅ | `Unknown` ❌ |
| **6. EmailField access** | `str` ✅ | `str` ✅ | `Unknown` ❌ |
| **7. IntegerField access** | `int` ✅ | `int` ✅ | `Unknown` ❌ |
| **8. Invalid attribute** | Error: "User" has no attribute | Error: Cannot access attribute | Warning: may be missing ⚠️ |
| **9. Invalid method** | Error: "Manager[User]" has no attribute | Error: Cannot access attribute | Warning: may be missing ⚠️ |
| **10. QuerySet chaining** | `QuerySet[User, User]` | `BaseManager[User]` | `Unknown \| QuerySet[User, User]` ✅ |

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

### ty + django-stubs (Real Stubs)

**Setup:**
```bash
# Option 1: Use uv to create a venv
uv venv
uv pip install django django-stubs

# Option 2: Use uvx (temporary environment)
uvx --with django-stubs --with django python -c "import django"
```

**Configuration (`ty.toml`):**
```toml
[environment]
python = ".venv/bin/python"  # Point to your venv
```

**Strengths:**
- ✅ Can read and use real django-stubs from Python environment
- ✅ Correctly infers `QuerySet[User, User]` (matches mypy!)
- ✅ Generic Manager/QuerySet types work
- ✅ Method chaining preserves type information
- ✅ Warns on possibly-missing attributes (not hard errors)

**Weaknesses:**
- ⚠️ Returns `Unknown | T` unions instead of just `T`
  - This suggests ty is uncertain about some type paths
  - Results in less precise types than mypy/pyright
- ❌ Field access returns `Unknown` instead of `str`/`int`
  - This is the biggest gap vs mypy/pyright
  - Likely needs descriptor protocol implementation
- ⚠️ Warnings instead of errors for invalid attributes
  - Less strict than mypy/pyright

## Implementation Gap Analysis

### What Works Today (with django-stubs)
✅ ty can read and use django-stubs from Python environment
✅ Generic `Manager[T]` and `QuerySet[T, T]` types are recognized
✅ Method chaining works correctly
✅ QuerySet types match mypy exactly

### Critical Gaps to Fix

**1. Unknown Unions (Highest Priority)**
- ty returns `Unknown | Manager[User]` instead of `Manager[User]`
- ty returns `Unknown | User` instead of `User`
- **Impact**: Less precise types, harder to catch errors
- **Root cause**: ty is uncertain about some type resolution paths
- **Fix needed**: Improve type narrowing/resolution logic

**2. Field Descriptor Protocol (High Priority)**
- Field access returns `Unknown` instead of actual types
- `user.name` should return `str`, not `Unknown`
- `user.age` should return `int`, not `Unknown`
- **Impact**: Major usability gap vs mypy/pyright
- **Fix needed**: Implement descriptor protocol support
  - Recognize `Field[T]` pattern
  - Return `T` when accessing field on model instance

**3. Stricter Error Reporting (Medium Priority)**
- Invalid attributes show warnings instead of errors
- Less strict than mypy/pyright
- **Impact**: May miss some bugs
- **Fix needed**: Make `possibly-missing-attribute` more strict for known types

### Future Enhancements (Lower Priority)

**Auto-synthesis (ty competitive advantage)**
- Auto-synthesize `.objects` manager (better than mypy!)
- Auto-synthesize `.DoesNotExist` exception
- Auto-add `id` field
- **Impact**: Better UX than mypy/pyright (no manual annotations needed)

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
