import pytest


@pytest.fixture()
def ok_no_scope():
    ...


@pytest.fixture(scope="module")
def ok_other_scope():
    ...


@pytest.fixture(scope="function")
def error():
    ...
