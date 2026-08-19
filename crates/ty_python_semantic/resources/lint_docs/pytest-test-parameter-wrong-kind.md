## What it does

Enforces that `pytest` tests accept keyword arguments.
Other kinds of arguments may cause errors.

## Examples

```python {data-mdtest="ignore"}
import pytest


# All good
@pytest.mark.parametrize("named, keyword_only", [(1, "2")])
def test_valid(named, *, keyword_only): ...


# Causes an error
@pytest.mark.parametrize("", [])
def test_invalid(
    pos_only, /, *args, **kwargs
): ...  # error: [pytest-test-argument-wrong-kind]
```
