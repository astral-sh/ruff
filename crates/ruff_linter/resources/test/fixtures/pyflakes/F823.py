my_dict = {}
my_var = 0


def foo():
    my_var += 1


def bar():
    global my_var
    my_var += 1


def baz():
    global my_var
    global my_dict
    my_dict[my_var] += 1


def dec(x):
    return x


@dec
def f():
    dec = 1
    return dec


class Class:
    def f(self):
        print(my_var)
        my_var = 1


class Class:
    my_var = 0

    def f(self):
        print(my_var)
        my_var = 1


import sys


def main():
    print(sys.argv)

    try:
        3 / 0
    except ZeroDivisionError:
        import sys

        sys.exit(1)


import sys


def main():
    print(sys.argv)

    for sys in range(5):
        pass


import requests_mock as rm


def requests_mock(requests_mock: rm.Mocker):
    print(rm.ANY)


import sklearn.base
import mlflow.sklearn


def f():
    import sklearn

    mlflow


# https://github.com/astral-sh/ruff/issues/30224
x = 1


def f():
    print(x)
    x = 2
    x = 3


def f(c):
    if c:
        x += 1
    if c:
        x += 1


def g():
    print(x)
    try:
        pass
    except ValueError as x:
        pass


def h():
    try:
        pass
    except ValueError as x:
        pass
    print(x)  # `UnboundLocalError` at runtime, but not reported, like pyflakes
