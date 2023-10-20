import pytest


@pytest.mark.usefixtures("a")
def test_something():  # Ok not fixture
    pass


@pytest.mark.usefixtures("a")
@pytest.fixture()
def my_fixture():  # Error before
    return 0


@pytest.fixture()
@pytest.mark.usefixtures("a")
def my_fixture():  # Error after
    return 0
