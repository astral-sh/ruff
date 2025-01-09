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


def f(a: Foo['''
Bar[
    Multi |
    Line]  # Comment''']): ...


def f(a: '''list[int]
	''' = []): ...


a: '''\\
list[int]''' = [42]


# TODO: These are valid too. String annotations are assumed to be enclosed in parentheses.
# https://github.com/astral-sh/ruff/issues/9467

def f(a: '''
	list[int]
	''' = []): ...


def f(a: Foo['''
    Bar
    [
    Multi |
    Line
    ]  # Comment''']): ...


a: '''list
[int]''' = [42]
