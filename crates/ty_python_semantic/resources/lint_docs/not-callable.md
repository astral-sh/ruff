## What it does

Checks for calls to non-callable objects.

## Why is this bad?

Calling a non-callable object will raise a `TypeError` at runtime.

## Example

```python
# TypeError: 'int' object is not callable
4()  # error
```

## See also

- `abstract-class-instantiation` reports attempts to instantiate a protocol class or a class with
    unimplemented abstract methods.

## Rule history

- v0.0.101: Renamed from `call-non-callable`, which remains as a deprecated alias for this rule.
