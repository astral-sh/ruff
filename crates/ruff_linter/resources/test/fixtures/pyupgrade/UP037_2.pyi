# https://github.com/astral-sh/ruff/issues/7102

def f(a: Foo['SingleLine  # Comment']): ...


def f(a: Foo['''Bar[
    Multi |
    Line]''']): ...


def f(a: Foo['''Bar[
    Multi |
    Line  # Comment
]''']): ...


def f(a: Foo['''Bar[
    Multi |
    Line]  # Comment''']): ...
