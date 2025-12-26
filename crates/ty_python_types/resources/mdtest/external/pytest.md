# pytest

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["pytest==9.0.1"]
```

## `pytest.fail`

Make sure that we recognize `pytest.fail` calls as terminal:

```py
import pytest

def some_runtime_condition() -> bool:
    return True

def test_something():
    if not some_runtime_condition():
        pytest.fail("Runtime condition failed")

        no_error_here_this_is_unreachable
```
