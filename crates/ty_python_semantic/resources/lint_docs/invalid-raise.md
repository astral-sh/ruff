Checks for `raise` statements that raise non-exceptions or use invalid
causes for their raised exceptions.

## Why is this bad?

Only subclasses or instances of `BaseException` can be raised.
For an exception's cause, the same rules apply, except that `None` is also
permitted. Violating these rules results in a `TypeError` at runtime.

## Examples

```python
def something():
    raise NameError


def cause() -> None:
    pass


def f():
    try:
        something()
    except NameError:
        # error: "Cannot raise object of type `Literal["oops!"]`"
        # error: "Cannot use object of type `def cause() -> None` as an exception cause"
        raise "oops!" from cause


def g():
    # error: "Cannot raise `NotImplemented`"
    # error: "Cannot use object of type `Literal[42]` as an exception cause"
    raise NotImplemented from 42
```

Use instead:

```python
def something():
    raise NameError


def f():
    try:
        something()
    except NameError as e:
        raise RuntimeError("oops!") from e


def g():
    raise NotImplementedError from None
```

## References

- [Python documentation: The `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#raise)
- [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)
