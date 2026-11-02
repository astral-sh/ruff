from pytest import yield_fixture
from pytest import yield_fixture as aliased


@yield_fixture()
def error_member_import():
    return 0


@aliased()
def error_aliased_member_import():
    return 0
