# `pytest-fixture-autouse`

```toml
lint.preview = true
lint.select = ["pytest-fixture-autouse"]
```

## Basic errors

```py
import pytest


@pytest.fixture(autouse=True)  # error: [pytest-fixture-autouse]
def my_autouse_fixture():
    pass


@pytest.fixture(scope="module", autouse=True)  # error: [pytest-fixture-autouse]
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

## Inline suppressions

A rule without a legacy code can be suppressed by name or by a blanket `noqa` comment.

```py
import pytest


@pytest.fixture(autouse=True)  # ruff: ignore[pytest-fixture-autouse]
def ignored_by_name():
    pass


@pytest.fixture(autouse=True)  # noqa
def ignored_by_blanket_noqa():
    pass
```
