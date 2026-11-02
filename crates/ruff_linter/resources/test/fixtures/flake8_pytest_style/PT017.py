import pytest


def test_ok():
    try:
        something()
    except Exception as e:
        something_else()

    with pytest.raises(ZeroDivisionError) as e:
        1 / 0
    assert e.value.message


def test_error():
    try:
        something()
    except Exception as e:
        assert e.message, "blah blah"


def test_error_with_multiple_exception_references():
    try:
        something()
    except Exception as e:
        assert len(e.args) == 1, e.args
