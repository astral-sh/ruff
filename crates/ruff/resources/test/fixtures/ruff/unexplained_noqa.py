"""Check that an unexplained noqa is reported.

This example should report two errors.
"""

# Errors.

something_evil()  # noqa

I = 1  # noqa: E741, F841

# Non-errors.

something_evil()  # noqa  # This is explained.

I = 1  # noqa: E741, F841  # This is also explained.
