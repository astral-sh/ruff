# Django Type Checking Testing

This directory contains files for testing and comparing Django type checking across different type checkers (mypy, pyright, and ty).

## Files

- **DJANGO_TYPE_CHECKER_COMPARISON.md** - Comprehensive comparison of mypy, pyright, and ty with Django
- **django_comparison_test.py** - Real Django code used to test all three type checkers
- **mypy.ini** - Mypy configuration for django-stubs plugin
- **test_settings.py** - Minimal Django settings required for testing

## Running the Comparison Tests

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

## Related Files

The mdtest suite for Django is located at:
- `crates/ty_python_semantic/resources/mdtest/django/`

These are the actual test files that ty runs to verify Django support.

## Key Findings

See `DJANGO_TYPE_CHECKER_COMPARISON.md` for detailed results and analysis.

**Summary:**
- ✅ ty can read and use real django-stubs
- ⚠️ ty returns `Unknown | T` unions (less precise than mypy/pyright)
- ❌ Field access returns `Unknown` (needs descriptor protocol)
- 🎯 Priority: Implement descriptor protocol for Field types
