import warnings
import pytest


def raise_user_warning(s):
    warnings.warn(s, UserWarning)
    return s


def test_ok():
    with pytest.warns(UserWarning):
        raise_user_warning("")


def test_error_trivial():
    pytest.warns(UserWarning, raise_user_warning, "warning")


def test_error_assign():
    s = pytest.warns(UserWarning, raise_user_warning, "warning")
    print(s)


def test_error_lambda():
    pytest.warns(UserWarning, lambda: warnings.warn("", UserWarning))
