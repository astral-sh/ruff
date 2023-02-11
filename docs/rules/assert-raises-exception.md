# assert-raises-exception (B017)

Derived from the **flake8-bugbear** linter.

## What it does
Checks for `self.assertRaises(Exception)`.

## Why is this bad?
`assertRaises(Exception)` can lead to your test passing even if the
code being tested is never executed due to a typo.

Either assert for a more specific exception (builtin or custom), use
`assertRaisesRegex` or the context manager form of `assertRaises`.

## Example
```python
self.assertRaises(Exception, foo)
```

Use instead:
```python
self.assertRaises(SomeSpecificException, foo)
```