class A:
    ...


class B(object):
    ...


class C(B, object):
    ...


class D(
    object,
    B,
):
    ...


def f():
    class E(object):
        ...


object = A


class F(object):
    ...
