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
    assert (
        something
        and something_else
        == """error
message
"""
    )

    # recursive case
    assert not (a or not (b or c))
    assert not (a or not (b and c))

    # detected, but no fix for messages
    assert something and something_else, "error message"
    assert not (something or something_else and something_third), "with message"
    # detected, but no fix for mixed conditions (e.g. `a or b and c`)
    assert not (something or something_else and something_third)


assert something  # OK
assert something and something_else  # Error
assert something and something_else and something_third  # Error


def test_multiline():
    assert something and something_else; x = 1

    x = 1; assert something and something_else

    x = 1; \
        assert something and something_else


# Regression test for: https://github.com/astral-sh/ruff/issues/7143
def test_parenthesized_not():
    assert not (
        self.find_graph_output(node.output[0])
        or self.find_graph_input(node.input[0])
        or self.find_graph_output(node.input[0])
    )

    assert (not (
        self.find_graph_output(node.output[0])
        or self.find_graph_input(node.input[0])
        or self.find_graph_output(node.input[0])
    ))

    assert (not self.find_graph_output(node.output[0]) or
            self.find_graph_input(node.input[0]))
