## What it does

Checks for various invalid `@overload` usages.

## Why is this bad?

The `@overload` decorator is used to define functions and methods that accepts different
combinations of arguments and return different types based on the arguments passed. This is mainly
beneficial for type checkers. But, if the `@overload` usage is invalid, the type checker may not be
able to provide correct type information.

## References

- [Python documentation: `@overload`](https://docs.python.org/3/library/typing.html#typing.overload)
