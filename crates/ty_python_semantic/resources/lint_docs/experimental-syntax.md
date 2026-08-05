## What it does

Checks for experimental syntax that is not part of the Python typing specification.

## Why is this bad?

Experimental syntax is specific to ty. It may be rejected by other type checkers and may never be
standardized, or be subject to breaking changes.

## Examples

```toml
[environment]
python-version = "3.14"
```

```python
class A: ...


class B: ...


def f(value: A & B) -> None: ...  # error: [experimental-syntax]
def g(value: ~A) -> None: ...  # error: [experimental-syntax]
```
