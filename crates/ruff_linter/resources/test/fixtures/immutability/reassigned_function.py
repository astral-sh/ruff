"""RUF066 - Single-assignment missing Final.

Should NOT warn - 'y' is reassigned.
"""


def f():
    """Function with a single assignment."""
    y = 2
    print(y)
    y = 3
