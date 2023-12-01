import pytest


def test_ok():
    with pytest.raises():
        pass


def test_error():
    with pytest.raises(UnicodeError):
        pass
