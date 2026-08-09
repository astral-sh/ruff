# Signatures

The examples in this file are just to check that the signature is parsed correctly. This mostly
checks that the correct argument kinds are used.

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["pytest==9.0.2"]
```

## Valid

```py
import pytest

# No args
@pytest.mark.parametrize("", [])
def _() -> None: ...

# One arg
@pytest.mark.parametrize("", [])
def _(x: int) -> None: ...

# Default args
@pytest.mark.parametrize("", [])
def _(x: int, y: str) -> None: ...

# Keyword-only arg
@pytest.mark.parametrize("", [])
def _(*, z: int) -> None: ...

# Mixed keyword args
@pytest.mark.parametrize("", [])
def _(a: bool, b: str, *, c: int, d: float) -> None: ...
```

## Invalid

```py
import pytest
from typing import Any

# Positional-only arg
@pytest.mark.parametrize("", [])
# error: [pytest-test-argument-wrong-kind] "Pytest tests only accept keyword arguments. `x` is a positional only argument."
def _(x: int, /) -> None: ...

# Variadic positional args
@pytest.mark.parametrize("", [])
# error: [pytest-test-argument-wrong-kind] "Pytest tests only accept keyword arguments. `*args` is a variadic positional argument."
def _(*args: Any) -> None: ...

# Variadic keyword args
@pytest.mark.parametrize("", [])
# error: [pytest-test-argument-wrong-kind] "Pytest tests only accept keyword arguments. `**kwargs` is a variadic keyword argument."
def _(**kwargs: Any) -> None: ...

# Optional arg
@pytest.mark.parametrize("foo", [1, 2, 3])
# error: [pytest-test-optional-argument] "Pytest tests ignore optional arguments. `bar` has a default value."
def _(foo: int, bar: int = 3) -> None: ...

# Combination
@pytest.mark.parametrize("", [])
# error: [pytest-test-argument-wrong-kind] "Pytest tests only accept keyword arguments. `x` is a positional only argument."
# error: [pytest-test-argument-wrong-kind] "Pytest tests only accept keyword arguments. `y` is a positional only argument."
# error: [pytest-test-optional-argument] "Pytest tests ignore optional arguments. `z` has a default value."
# error: [pytest-test-argument-wrong-kind] "Pytest tests only accept keyword arguments. `**kwargs` is a variadic keyword argument."
def _(x: int, y: int, /, z: int = 8, **kwargs) -> None: ...
```
