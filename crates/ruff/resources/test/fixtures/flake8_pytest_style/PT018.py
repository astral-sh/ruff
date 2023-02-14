import pytest


def test_ok():
    assert something
    assert something or something_else
    assert something or something_else and something_third
    assert not (something and something_else)


def test_error():
    assert something and something_else
    assert something and something_else and something_third
    assert something and not something_else
    assert something and (something_else or something_third)
    assert not (something or something_else)
    assert not (something or something_else or something_third)
    assert not (something or something_else and something_third)
