import pytest


@pytest.mark.usefixtures("a")
def test_ok():
    pass


@pytest.mark.foo()
def test_ok_another_mark_with_parens():
    pass


@pytest.mark.foo
def test_ok_another_mark_no_parens():
    pass


@pytest.mark.usefixtures()
def test_error_with_parens():
    pass


@pytest.mark.usefixtures
def test_error_no_parens():
    pass
