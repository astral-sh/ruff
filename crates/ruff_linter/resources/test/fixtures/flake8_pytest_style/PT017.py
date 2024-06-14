import pytest


def test_ok():
    try:
        something()
    except Exception as e:
        something_else()

    with pytest.raises(ZeroDivisionError) as e:
        1 / 0
    assert e.value.message


def allow_exception_in_message():
    x = 0
    try:
        1 / x
    except ZeroDivisionError as e:
        assert x == 0, e


def test_error():
    try:
        something()
    except Exception as e:
        assert e.message, "blah blah"
