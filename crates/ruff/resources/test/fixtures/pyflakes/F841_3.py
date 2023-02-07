"""Test case for autofixing F841 violations."""


def f():
    x = 1
    y = 2

    z = 3
    print(z)


def f():
    x: int = 1
    y: int = 2

    z: int = 3
    print(z)


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
    try:
        1 / 0
    except ValueError as x1:
        pass

    try:
        1 / 0
    except (ValueError, ZeroDivisionError) as x2:
        pass


def f(a, b):
    x = (
        a()
        if a is not None
        else b
    )

    y = \
        a() if a is not None else b


def f(a, b):
    x = (
        a
        if a is not None
        else b
    )

    y = \
        a if a is not None else b
