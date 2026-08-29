## What it does

Enforces that the `request` keyword is not used during parameterization. `pytest` passes the
`request` argname to all functions, and raises an error on parameters of fixtures.

## Extra Information

For more information, see the relevant Pytest documentation:
<https://docs.pytest.org/en/stable/how-to/fixtures.html#fixture-parametrize>.

## Examples

```python {data-mdtest="ignore"}
import pytest
from _pytest.fixtures import TopRequest


@pytest.mark.parametrize("foo, request", [(1, 2)])  # error: [pytest-request-keyword]
def test_foo(foo: int, request: Request) -> None: ...
```
