import pytest


def func(a, b):
    return a / b


def test_ok():
    with pytest.raises(ValueError):
        raise ValueError


def test_ok_as():
    with pytest.raises(ValueError) as excinfo:
        raise ValueError


def test_error_trivial():
    pytest.raises(ZeroDivisionError, func, 1, b=0)


def test_error_match():
    pytest.raises(ZeroDivisionError, func, 1, b=0).match("division by zero")


def test_error_assign():
    excinfo = pytest.raises(ZeroDivisionError, func, 1, b=0)


def test_error_kwargs():
    pytest.raises(func=func, expected_exception=ZeroDivisionError)


def test_error_multi_statement():
    excinfo = pytest.raises(ValueError, int, "hello")
    assert excinfo.match("^invalid literal")


def test_error_lambda():
    pytest.raises(ZeroDivisionError, lambda: 1 / 0)
