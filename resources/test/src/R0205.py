class A:
    ...


class B(object):
    ...


class C(B, object):
    ...


def f():
    class D(object):
        ...


object = A


class E(object):
    ...
