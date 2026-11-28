"""Test: the fix removes the unused import when a name is imported more than once.

See: https://github.com/astral-sh/ruff/issues/29992
"""


def f():
    # Only the alias is unused, so `join` should be kept.
    from os.path import join, join as path_join

    print(join("a", "b"))


def f():
    # Only the plain import is unused, so `path_join` should be kept.
    from os.path import join, join as path_join

    print(path_join("a", "b"))


def f():
    # Only the first alias is unused.
    from os.path import join as a, join as b

    print(b("a", "b"))


def f():
    # Plain `import` statements with the same module twice.
    import os.path, os.path as osp

    print(os.path.sep)
