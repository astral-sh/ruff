## What it does

Check for duplicated argnames when using `pytest.mark.parametrize`.
This will cause `pytest` to crash when executing the test.

## Examples

```python {data-mdtest="ignore"}
import pytest


# In the same parametrization.
@pytest.mark.parametrize("x, y, x", [(1, -1, 1)])  # error: [pytest-duplicate-argnames]
def test_duplicate(x: int, y: int) -> None: ...


# Or in separate ones.
@pytest.mark.parametrize("x, y", [(1, 2)])
@pytest.mark.parametrize("z, y", [(3, 4)])  # error: [pytest-duplicate-argnames]
def test_duplicate(x: int, y: int, z: int) -> None: ...
```
