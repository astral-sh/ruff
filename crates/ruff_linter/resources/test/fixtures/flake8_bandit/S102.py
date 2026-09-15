def fn():
    # Error
    exec('x = 2')

exec('y = 3')


## https://github.com/astral-sh/ruff/issues/15442
def _():
    from builtins import exec
    exec('')  # Error

def _():
    from builtin import exec
    exec('')  # No error


## https://github.com/astral-sh/ruff/issues/28011
## References to `exec` are only flagged in preview.
list(map(exec, ["x = 2"]))  # Error in preview

my_exec = exec  # Error in preview


def _():
    from builtin import exec
    my_exec = exec  # No error
