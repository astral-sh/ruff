# blind-except (BLE001)

Derived from the **flake8-blind-except** linter.

## What it does
Checks that `Exception` is not being caught in `try...except` statements.

## Why is this bad?
Catching `Exception` includes `AssertionError`, `ImportError`, `NameError`, `AttributeError`, `SyntaxError` and others that may indicate significant problems in your source code rather than the behavior of your application. Care should be taken to avoid catching broad exceptions that may include many unexpected errors.

## Example
```python
try:
    assert False, "You probably don't want to catch AssertionErrors."
except Exception:
    print("But you will catch them.")
```

Use instead:
```python
try:
    assert False, "You probably don't want to catch AssertionErrors."
except MoreSpecificException:
    print("And now you won't.")
```

Or:
```python
try:
   assert False, "You probably don't want to catch AssertionErrors."
except Exception:
   logger.Exception("Catch and log errors in a top level module.")
   raise
```

## References
- [Python Exception Hierarchy](https://docs.python.org/3/library/exceptions.html#exception-hierarchy)
- [Google Python Style Guide on Exceptions](https://google.github.io/styleguide/pyguide.html#24-exceptions)