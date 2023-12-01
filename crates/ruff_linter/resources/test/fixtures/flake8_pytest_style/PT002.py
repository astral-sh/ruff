import pytest


@pytest.fixture()
def my_fixture():  # OK no args
    return 0


@pytest.fixture(scope="module")
def my_fixture():  # OK only kwargs
    return 0


@pytest.fixture("module")
def my_fixture():  # Error only args
    return 0


@pytest.fixture("module", autouse=True)
def my_fixture():  # Error mixed
    return 0
