"""Test: annotated global."""


n: int


def f():
    print(n)


def g():
    global n
    n = 1


g()
f()
