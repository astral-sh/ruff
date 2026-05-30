import pytest

# Positive cases (violations)
@pytest.fixture(autouse=True)
def my_autouse_fixture():
    pass

@pytest.fixture(scope="module", autouse=True)
def my_scoped_autouse_fixture():
    pass

# Negative cases (no violations)
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
