# `pytest-fixture-autouse` (`RUF076`)

```toml
lint.preview = true
# lint.select = ["RUF076"]
```

## Basic errors

```py
import pytest


@pytest.fixture(autouse=True)  # TODO: snapshot: pytest-fixture-autouse
def my_autouse_fixture():
    pass


@pytest.fixture(scope="module", autouse=True)  # TODO: error: [pytest-fixture-autouse]
def my_scoped_autouse_fixture():
    pass
```

## No errors

```py
import pytest


@pytest.fixture()
def standard_fixture():
    pass


@pytest.fixture(autouse=False)
def explicit_false_autouse_fixture():
    pass


@pytest.fixture
def decorator_no_arguments():
    pass


# Not a pytest fixture
def not_a_fixture(autouse=True):
    pass
```
