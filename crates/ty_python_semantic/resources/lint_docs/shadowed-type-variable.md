## What it does

Checks for type variables in nested generic classes or functions that shadow type variables
from an enclosing scope.

## Why is this bad?

Shadowing type variables makes the code confusing and is disallowed by the typing spec.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
class Outer[T]:
    # `T` is already used by `Outer`
    class Inner[T]: ...  # error

    # `T` is already used by `Outer`
    def method[T](self, x: T) -> T:  # error
        return x
```

## References

- [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)
