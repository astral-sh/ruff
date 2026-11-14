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

# https://github.com/astral-sh/ruff/issues/28011
list(map(exec, ["hi"]))  # Preview: Error
foo = exec  # Preview: Error

def _():
    def exec(): ...

    list(map(exec, []))  # Shadowed -- No error
