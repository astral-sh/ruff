## What it does

Checks that the argnames passed to `pytest.mark.parametrize` are valid. If not, the tests will not
run.

## Examples

```python {data-mdtest="ignore"}
import pytest


# Here, you should use `"x, y"` instead.
@pytest.mark.parametrize("x y", [(1, -1)])  # error: [pytest-invalid-argnames-literal]
def test_x_and_y(x: int, y: int) -> None: ...
```
