import pytest


@pytest.mark.parametrize("params", [(1,), (2,)])
def test_foo(params):
    assert isinstance(params, int)
