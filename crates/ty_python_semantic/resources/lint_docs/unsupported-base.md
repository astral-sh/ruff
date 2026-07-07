## What it does

Checks for class definitions that have bases which are unsupported by ty.

## Why is this bad?

If a class has a base that is an instance of a complex type such as a union type,
ty will not be able to resolve the [method resolution order] (MRO) for the class.
This will lead to an inferior understanding of your codebase and unpredictable
type-checking behavior.

## Examples

```python
import datetime


class A: ...


class B: ...


if datetime.date.today().weekday() != 6:
    C = A
else:
    C = B


class D(C): ...  # error: [unsupported-base]
```

[method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
