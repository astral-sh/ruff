import pytest


def test_ok():
    with pytest.warns():
        pass


def test_error():
    with pytest.warns(UserWarning):
        pass
