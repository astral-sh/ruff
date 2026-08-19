## What it does

Checks that the types of arguments to `pytest` tests.
This is currently not exhaustive, but can spot errors in `pytest.mark.parametrize` calls.

## Examples

```python {data-mdtest="ignore"}
import pytest


# In this test case, the third value of `y` should be a `str`, not `None`.
@pytest.mark.parametrize(
    ("x", "y"),
    [
        (1, "3"),
        (3, "4"),
        (5, None),  # error: [pytest-param-mismatched-type]
    ],
)
def test_int_and_string(x: int, y: str) -> None: ...
```
