import socket

import pytest


def test_ok():
    with pytest.raises(ValueError, match="Can't divide by 0"):
        raise ValueError("Can't divide by 0")


def test_ok_different_error_from_config():
    with pytest.raises(ZeroDivisionError):
        raise ZeroDivisionError("Can't divide by 0")


def test_error_no_argument_given():
    with pytest.raises(ValueError):
        raise ValueError("Can't divide 1 by 0")

    with pytest.raises(socket.error):
        raise ValueError("Can't divide 1 by 0")


def test_error_match_is_empty():
    with pytest.raises(ValueError, match=None):
        raise ValueError("Can't divide 1 by 0")

    with pytest.raises(ValueError, match=""):
        raise ValueError("Can't divide 1 by 0")

    with pytest.raises(ValueError, match=f""):
        raise ValueError("Can't divide 1 by 0")
