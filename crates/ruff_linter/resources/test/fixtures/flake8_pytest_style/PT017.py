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
