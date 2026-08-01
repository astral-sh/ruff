## What it does

Detects invalid `super()` calls where implicit arguments like the enclosing class or first method argument are unavailable.

## Why is this bad?

When `super()` is used without arguments, Python tries to find two things:
the nearest enclosing class and the first argument of the immediately enclosing function (typically self or cls).
If either of these is missing, the call will fail at runtime with a `RuntimeError`.

## Examples

```python
# no enclosing class or function found
super()  # error


def func():
    # no enclosing class or first argument exists
    super()  # error


class A:
    # no enclosing function to provide the first argument
    f = super()  # error

    def method(self):
        def nested():
            # first argument does not exist in this nested function
            super()  # error

        # first argument does not exist in this lambda
        lambda: super()  # error

        # argument is not available in generator expression
        (super() for _ in range(10))  # error

        super()  # okay! both enclosing class and first argument are available
```

## References

- [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)
