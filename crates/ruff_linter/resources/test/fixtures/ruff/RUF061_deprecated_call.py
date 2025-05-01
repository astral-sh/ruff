import warnings
import pytest


def raise_deprecation_warning(s):
    warnings.warn(s, DeprecationWarning)
    return s


def test_ok():
    with pytest.deprecated_call():
        raise_deprecation_warning("")


def test_error_trivial():
    pytest.deprecated_call(raise_deprecation_warning, "deprecated")


def test_error_assign():
    s = pytest.deprecated_call(raise_deprecation_warning, "deprecated")
    print(s)


def test_error_lambda():
    pytest.deprecated_call(lambda: warnings.warn("", DeprecationWarning))
