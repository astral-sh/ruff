"""Regression test for: https://github.com/astral-sh/ruff/issues/29992"""


# The second binding (`join as path_join`) is unused, so only it should be removed.
def f1():
    from os.path import join, join as path_join

    print(join("a", "b"))


# The first binding (`join`) is unused, so only it should be removed.
def f2():
    from os.path import join, join as path_join

    print(path_join("a", "b"))


# The first binding (`join as path_join`) is unused, so only it should be removed.
def f3():
    from os.path import join as path_join, join

    print(join("a", "b"))


# Both aliased: second binding (`os as os2`) is unused.
def f4():
    import os as os1, os as os2

    print(os1.name)


# Both aliased: first binding (`os as os1`) is unused.
def f5():
    import os as os1, os as os2

    print(os2.name)
