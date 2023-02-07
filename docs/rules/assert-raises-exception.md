# assert-raises-exception (B017)

### What it does
Checks for the use of `assertRaises(Exception)`.

### Why is this bad?
`assertRaises(Exception)` can lead to your test passing even if the
code being tested is never executed (e.g., due to a typo).

Assert for a more specific exception (builtin or custom), use
`assertRaisesRegex` or the context manager form of `assertRaises`.

### Example
```python
self.assertRaises(Exception, foo)
```

Use instead:
```python
self.assertRaises(SomeSpecificException, foo)
```
