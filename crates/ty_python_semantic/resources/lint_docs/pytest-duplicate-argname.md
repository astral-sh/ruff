## What it does

Checks that there are no duplicate argnames in `pytest.mark.parametrize`. If there are, the tests
will not run.

## Examples

```python {data-mdtest="ignore"}
import pytest


# It must not be repeated within the same parametrization.
@pytest.mark.parametrize("x, x, y", [(1, 1, 3)])  # error: [pytest-duplicate-argname]
def test_x_and_y(x: int, y: int) -> None: ...


# Or between different ones.
@pytest.mark.parametrize("a", [1])
@pytest.mark.parametrize("b", [2])
@pytest.mark.parametrize("b", [3])  # error: [pytest-duplicate-argname]
def test_a_anb_y(a: int, b: int) -> None: ...
```
