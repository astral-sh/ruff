import pytest


def test_ok():
    assert something
    assert something or something_else
    assert something or something_else and something_third
    assert not (something and something_else)
    assert something, "something message"
    assert something or something_else and something_third, "another message"


def test_error():
    assert something and something_else
    assert something and something_else and something_third
    assert something and not something_else
    assert something and (something_else or something_third)
    assert not something and something_else
    assert not (something or something_else)
    assert not (something or something_else or something_third)
    assert something and something_else == """error
    message
    """

    # recursive case
    assert not (a or not (b or c))
    assert not (a or not (b and c))

    # detected, but no autofix for messages
    assert something and something_else, "error message"
    assert not (something or something_else and something_third), "with message"
    # detected, but no autofix for mixed conditions (e.g. `a or b and c`)
    assert not (something or something_else and something_third)
    # detected, but no autofix for parenthesized conditions
    assert (
        something
        and something_else
        == """error
message
"""
    )
