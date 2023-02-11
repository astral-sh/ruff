# any-type (ANN401)

Derived from the **flake8-annotations** linter.

## What it does
Checks that an expression is annotated with a more specific type than
`Any`.

## Why is this bad?
`Any` is a special type indicating an unconstrained type. When an
expression is annotated with type `Any`, type checkers will allow all
operations on it.

It's better to be explicit about the type of an expression, and to use
`Any` as an "escape hatch" only when it is really needed.

## Example
```python
def foo(x: Any):
    ...
```

Use instead:
```python
def foo(x: int):
    ...
```

## References
* [PEP 484](https://www.python.org/dev/peps/pep-0484/#the-any-type)
* [`typing.Any`](https://docs.python.org/3/library/typing.html#typing.Any)
* [Mypy: The Any type](https://mypy.readthedocs.io/en/stable/kinds_of_types.html#the-any-type)