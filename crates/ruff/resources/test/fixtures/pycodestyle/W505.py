#!/usr/bin/env python3
"""Here's a top-level docstring that's over the limit."""


def f():
    """Here's a docstring that's also over the limit."""

    x = 1  # Here's a comment that's over the limit, but it's not standalone.

    # Here's a standalone comment that's over the limit.

    print("Here's a string that's over the limit, but it's not a docstring.")

# E501 & not W505
looooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong_var = 10

# W505 & not E501
"This is also considered a docstring, and is over the limit. This is also considered a docstring, and is over the limit. This is also considered a docstring, and is over the limit. This is also considered a docstring, and is over the limit."
