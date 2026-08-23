import pytest as other_name


@other_name.yield_fixture()
def error_aliased_module():
    return 0
