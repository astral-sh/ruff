import pytest


@pytest.fixture()
def ok_no_parameters():
    return 0


@pytest.fixture
def ok_without_parens():
    return 0


@pytest.yield_fixture()
def error_without_parens():
    return 0


@pytest.yield_fixture
def error_with_parens():
    return 0
