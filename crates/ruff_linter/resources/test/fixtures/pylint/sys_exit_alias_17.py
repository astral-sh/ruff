# `sys` is only bound if the branch executes, so the fix must not rely on it.
# https://github.com/astral-sh/ruff/issues/4419
if False:
    import sys

exit(1)
