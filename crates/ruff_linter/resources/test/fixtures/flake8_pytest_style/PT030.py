import pytest
from foo import FooWarning


def test_ok():
    with pytest.warns(UserWarning, match="Can't divide by 0"):
        pass


def test_ok_different_error_from_config():
    with pytest.warns(EncodingWarning):
        pass


def test_error_no_argument_given():
    with pytest.warns(UserWarning):
        pass

    with pytest.warns(expected_warning=UserWarning):
        pass

    with pytest.warns(FooWarning):
        pass


def test_error_match_is_empty():
    with pytest.warns(UserWarning, match=None):
        pass

    with pytest.warns(UserWarning, match=""):
        pass

    with pytest.warns(UserWarning, match=f""):
        pass

def test_ok_match_t_string():
    with pytest.warns(UserWarning, match=t""):
        pass
