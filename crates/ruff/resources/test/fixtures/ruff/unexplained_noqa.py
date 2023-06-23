"""Check that an unexplained noqa is reported.

This example should report four errors.
"""

# Errors.

something_evil()  # noqa

foo == ""  # noqa: PLC1901

I = 1  # noqa: E741, F841

assert False # noqa:              B011

def func():  # noqa: ANN
    ...

# Non-errors.

something_evil()  # noqa  # Works with blank noqa.

foo == ""  # noqa: PLC1901  # Works with code after noqa.

I = 1  # noqa: E741, F841  # Works with multiple codes.

assert False # noqa:              B011  # Works with eading whitespace before code.

def func():  # noqa: ANN  # Works with codes that don't end in a number.
    ...
