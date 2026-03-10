# Regression test for https://github.com/astral-sh/ty/issues/3011

x = None
for _ in range(10):
    x = 0 if x is None else x + 1