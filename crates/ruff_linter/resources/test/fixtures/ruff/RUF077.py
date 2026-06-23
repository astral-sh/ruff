"""Regression cases for RUF077 (no blank line at function start)."""


def plain_function_with_blank():
    """Function with a docstring — D201/D202 territory, not us."""

    body = "should be no blank above this line"


def function_with_blank():
    x = 1

    y = 2
    return x + y


def no_blank_ok():
    x = 1
    y = 2
    return x + y


def only_pass_ok():
    pass


async def async_function_with_blank():

    return "done"


async def async_no_blank_ok():
    return "done"