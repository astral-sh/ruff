import pytest
from pytest import fixture
from pytest import fixture as aliased


# `import pytest`


@pytest.fixture
def no_parentheses():
    return 42


@pytest.fixture()
def parentheses_no_params():
    return 42


@pytest.fixture(scope="module")
def parentheses_with_params():
    return 42


@pytest.fixture(

)
def parentheses_no_params_multiline():
    return 42


# `from pytest import fixture`


@fixture
def imported_from_no_parentheses():
    return 42


@fixture()
def imported_from_parentheses_no_params():
    return 42


@fixture(scope="module")
def imported_from_parentheses_with_params():
    return 42


@fixture(

)
def imported_from_parentheses_no_params_multiline():
    return 42


# `from pytest import fixture as aliased`


@aliased
def aliased_no_parentheses():
    return 42


@aliased()
def aliased_parentheses_no_params():
    return 42


@aliased(scope="module")
def aliased_parentheses_with_params():
    return 42


@aliased(

)
def aliased_parentheses_no_params_multiline():
    return 42
