"""Check that an unexplained noqa is reported.

This example should report four errors.
"""

# Errors.

something_evil()  # noqa

foo == ""  # noqa: PLC1901

I = 1  # noqa: E741, F841

assert False # noqa:              B011

# Non-errors.

something_evil()  # noqa  # This is explained.

foo == ""  # noqa: PLC1901  # Check for empty string, not just falsiness.

I = 1  # noqa: E741, F841  # This is also explained.

assert False # noqa:              B011  # This is also explained.
