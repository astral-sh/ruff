# Regression test for https://github.com/astral-sh/ty/issues/4793.

def wrap(paths, start):
    values = (start,)
    for _ in paths:
        values = (list(values),)
