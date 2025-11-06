# Django Type Checking Testing

This directory contains files for comparing Django type checking across different type checkers (mypy, pyright, and ty) **using real Django and real stubs**.

**Note**: The mdtest files in `crates/ty_python_semantic/resources/mdtest/django/` use **inline custom stubs** and don't require django-stubs. This comparison directory tests with real Django to see how ty performs with actual django-stubs in practice.

## Files

- **django_comparison_test.py** - Real Django code used to test all three type checkers
- **mypy.ini** - Mypy configuration for django-stubs plugin
- **test_settings.py** - Minimal Django settings required for testing

## Test Code

The comparison uses this Django code:

```python
from django.db import models
from django.db.models.manager import Manager

class User(models.Model):
    name = models.CharField(max_length=100)
    email = models.EmailField()
    age = models.IntegerField()
    objects = Manager["User"]()  # Required for type checking

# Test cases
reveal_type(User)                      # Model class type
reveal_type(User.objects)              # Manager type
reveal_type(User.objects.all())        # QuerySet type
reveal_type(User.objects.get(id=1))    # Model instance
reveal_type(user.name)                 # CharField access
# ... and more
```

## Running Comparison Tests

### Test with mypy + django-stubs

```bash
uvx --with django-stubs[compatible-mypy] mypy django_comparison_test.py
```

### Test with pyright + django-types

```bash
uvx --with django --with django-types pyright django_comparison_test.py
```

### Test with ty + django-stubs

```bash
# Create a venv with django-stubs
uv venv
uv pip install django django-stubs

# Run ty (configure python path in ty.toml if needed)
cargo run -p ty -- check django_comparison_test.py
```

## Results Comparison

| Test                    | Mypy + django-stubs                       | Pyright + django-types | ty + django-stubs                    |
| ----------------------- | ----------------------------------------- | ---------------------- | ------------------------------------ |
| **Model class type**    | `def (*args: Any, **kwargs: Any) -> User` | `type[User]`           | `<class 'User'>` ✅                  |
| **Manager type**        | `Manager[User]`                           | `BaseManager[User]`    | `Unknown \| Manager[User]` ⚠️        |
| **QuerySet type**       | `QuerySet[User, User]`                    | `BaseManager[User]`    | `Unknown \| QuerySet[User, User]` ✅ |
| **Model instance**      | `User`                                    | `User`                 | `Unknown \| User` ⚠️                 |
| **CharField access**    | `str` ✅                                  | `str` ✅               | `Unknown \| str` ⚠️                  |
| **EmailField access**   | `str` ✅                                  | `str` ✅               | `Unknown \| str` ⚠️                  |
| **IntegerField access** | `int` ✅                                  | `int` ✅               | `Unknown \| int` ⚠️                  |
| **Invalid attribute**   | Error: has no attribute                   | Error: Cannot access   | Warning: may be missing ⚠️           |
| **QuerySet chaining**   | `QuerySet[User, User]`                    | `BaseManager[User]`    | `Unknown \| QuerySet[User, User]` ✅ |

## Key Findings

### ty Strengths (with custom stubs in mdtest)

- ✅ Descriptor protocol **partially works** - returns `Unknown | str` for CharField
- ✅ Generic Manager/QuerySet types work with manual annotations
- ✅ Method chaining preserves type information

### ty Gaps (discovered in mdtest)

- ⚠️ Returns `Unknown | T` unions instead of just `T` (descriptor protocol works but needs type narrowing)
- ❌ Self type not supported - `Manager[Self]` fails with "Expected `Model`, found `typing.Self`" (needs PEP 673)

### Real django-stubs Testing

When tested with real Django code and django-stubs (django_comparison_test.py):

- ✅ Can read and use real django-stubs from Python environment
- ⚠️ Same Unknown union behavior as with custom stubs
- ⚠️ Warnings instead of errors for invalid attributes (less strict than mypy/pyright)

### Mypy + django-stubs

- ✅ Most comprehensive Django support via mypy plugin
- ✅ Full field type inference (CharField → str, IntegerField → int)
- ⚠️ Requires explicit `objects = Manager["User"]()` annotation

### Pyright + django-types

- ✅ Good Django support with simpler type model
- ✅ Full field type inference
- ⚠️ Uses `BaseManager[T]` instead of `QuerySet[T]` for query results
- ⚠️ Requires explicit `objects = Manager["User"]()` annotation

## Implementation Priorities

### 1. Self Type Support (Blocker)

- `Manager[Self]` fails with "Expected `Model`, found `typing.Self`"
- **Impact**: Blocks most Manager/QuerySet type inference
- **Fix needed**: Implement PEP 673 Self type support

### 2. Unknown Unions (High Priority)

- ty returns `Unknown | str` instead of `str` for field access
- ty returns `Unknown | Manager[User]` instead of `Manager[User]`
- **Impact**: Less precise types, harder to catch errors
- **Fix needed**: Improve type narrowing to remove Unknown from unions
- **Good news**: Descriptor protocol partially works! Just needs refinement.

### 3. Auto-synthesis (Competitive Advantage)

- Auto-synthesize `.objects` manager (better than mypy!)
- Users wouldn't need manual `objects = Manager["User"]()` annotations
- **Impact**: Better UX than mypy/pyright

## Related Files

The mdtest suite for Django is located at:

- `crates/ty_python_semantic/resources/mdtest/django/`

These are the actual test files that ty runs to verify Django support.
