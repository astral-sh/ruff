import pytest


def test_ok():
    with pytest.raises():
        pass


def test_error():
    with pytest.raises(UnicodeError):
        pass

def test_match_only():
    with pytest.raises(match="some error message"):
        pass

def test_check_only():
    with pytest.raises(check=lambda e: True):
        pass

def test_match_and_check():
    with pytest.raises(match="some error message", check=lambda e: True):
        pass
