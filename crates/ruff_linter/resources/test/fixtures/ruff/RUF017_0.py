x = [1, 2, 3]
y = [4, 5, 6]

# RUF017
sum([x, y], start=[])
sum([x, y], [])
sum([[1, 2, 3], [4, 5, 6]], start=[])
sum([[1, 2, 3], [4, 5, 6]], [])
sum([[1, 2, 3], [4, 5, 6]],
    [])

# OK
sum([x, y])
sum([[1, 2, 3], [4, 5, 6]])


# Regression test for: https://github.com/astral-sh/ruff/issues/7059
def func():
    import functools, operator

    sum([x, y], [])


# Regression test for: https://github.com/astral-sh/ruff/issues/7718
def func():
    sum((factor.dims for factor in bases), [])
