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
def f():
    print(my_var)
    my_var = 1
    my_var = 2


def f():
    my_var += 1
    my_var += 1


def f():
    if True:
        my_var += 1
    if False:
        my_var += 1


# OK: the read comes after the first local binding.
def f():
    my_var = 1
    print(my_var)
    my_var = 2


# `except ... as my_var` makes `my_var` local, and it's unbound again after the
# handler, so this read always raises an `UnboundLocalError`.
def f():
    try:
        pass
    except Exception as my_var:
        pass
    print(my_var)
