#!/usr/bin/env python3
"""Here's a top-level docstring that's over the limit."""


def f1():
    """Here's a docstring that's also over the limit."""

    x = 1  # Here's a comment that's over the limit, but it's not standalone.

    # Here's a standalone comment that's over the limit.

    x = 2
    # Another standalone that is preceded by a newline and indent token and is over the limit.

    print("Here's a string that's over the limit, but it's not a docstring.")


"This is also considered a docstring, and is over the limit."


def f2():
    """Here's a multi-line docstring.

    It's over the limit on this line, which isn't the first line in the docstring.
    """


def f3():
    """Here's a multi-line docstring.

    It's over the limit on this line, which isn't the first line in the docstring."""
