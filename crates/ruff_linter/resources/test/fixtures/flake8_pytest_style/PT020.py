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


@pytest.yield_fixture(scope="module", name="my_fixture")
def error_with_arguments():
    return 0


@pytest.yield_fixture()  # comment
def error_with_comment():
    return 0


class TestClass:
    @pytest.yield_fixture()
    def error_in_class(self):
        return 0


@(
    pytest
    # comment
    .yield_fixture
)
def error_with_comment_in_reference():
    return 0
