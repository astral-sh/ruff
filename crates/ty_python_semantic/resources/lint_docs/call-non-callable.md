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

- `instantiate-abstract-class` reports attempts to instantiate a protocol class or a class with
    unimplemented abstract methods.
