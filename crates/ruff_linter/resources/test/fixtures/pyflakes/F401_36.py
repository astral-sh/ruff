"""Regression test for https://github.com/astral-sh/ruff/issues/29992"""

from os.path import join, join as path_join

print(join("a", "b"))


from os.path import split as path_split, split

print(path_split("a/b"))
