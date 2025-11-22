"""RUF066 - Single-assignment missing Final.

Should warn - 'y' is assigned exactly once but not annotated with Final.
"""


def f():
    """Function with a single assignment."""
    y = 2
    print(y)
