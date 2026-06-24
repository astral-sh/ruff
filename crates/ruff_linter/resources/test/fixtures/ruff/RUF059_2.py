"""Test case for fixing RUF059 violations."""


def f():
    with foo() as x1:
        pass

    with foo() as (x2, y2):
        pass

    with (foo() as x3, foo() as y3, foo() as z3):
        pass


def f():
    (x1, y1) = (1, 2)
    (x2, y2) = coords2 = (1, 2)
    coords3 = (x3, y3) = (1, 2)


def f():
    with Nested(m) as (x, y):
        pass


def f():
    toplevel = (a, b) = lexer.get_token()


def f():
    (a, b) = toplevel = lexer.get_token()
