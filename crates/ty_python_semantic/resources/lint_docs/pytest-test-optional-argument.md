## What it does

Checks that there are no positional arguments in Pytest tests.

## Why is this bad?

Positional arguments are ignored.

## Examples

```python {data-mdtest="ignore"}
import pytest


@pytest.mark.parametrize("x", [(1, -1)])
def test_negation(x: int, zero=0) -> None:  # error: [pytest-test-optional-argument]
    assert -x == zero - x


# Instead, use a local variable.
@pytest.mark.parametrize("x", [(1, -1)])
def test_negation(x: int) -> None:
    zero = 0
    assert -x == zero - x
```
