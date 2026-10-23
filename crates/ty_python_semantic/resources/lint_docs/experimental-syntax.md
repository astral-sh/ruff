## What it does

Checks for experimental syntax that is not part of the Python typing specification.

## Why is this bad?

Experimental syntax is specific to ty. It may be rejected by other type checkers and may never be
standardized, or be subject to breaking changes. There are also tools and libraries that inspect
and/or evaluate type annotations at runtime, including stringized annotations (e.g. Pydantic,
typeguard or beartype). Using experimental syntax may lead to runtime errors in this context.

Conversely, if you are only using ty as your type checker, and if you are not relying on runtime
inspection of type annotations, you can safely ignore this rule.

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
