## What it does

Checks for malformed generic class declarations, such as a class that declares type parameters both
in brackets and through a `Generic[...]` base.

## Why is this bad?

There are several requirements that you must follow when defining a generic class. Many of these
result in `TypeError` being raised at runtime if they are violated.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
from typing_extensions import Generic, TypeVar

T = TypeVar("T")


# class uses both PEP-695 syntax and legacy syntax
class C[U](Generic[T]): ...  # error
```

## References

- [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)

## Rule history

- v0.0.102: Split out `invalid-type-variable-variance`, `inconsistent-generic-bases`, and
    `invalid-type-parameter-order`. Defaults that reference out-of-scope type variables now use
    `invalid-type-variable-default`.
